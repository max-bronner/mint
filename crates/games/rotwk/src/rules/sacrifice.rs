use mint_sage_modules::effects::{Effect, Names};
use mint_sim::StateHasher;

use super::*;

const MODULE: &str = "BloodthirstyUpdate";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sacrificing {
    pub(crate) victim: Entity,
    pub(crate) kills: u32,
}

impl Component for Sacrificing {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.victim.hash_into(hasher);
        hasher.write_u32(self.kills);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sacrificed {
    pub(crate) by: Entity,
}

impl Component for Sacrificed {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.by.hash_into(hasher);
    }
}

fn names(list: &[&str]) -> Names {
    list.iter()
        .map(|name| std::sync::Arc::<str>::from(*name))
        .collect::<Vec<_>>()
        .into()
}

impl RotwkRules {
    fn sacrifice_home(world: &World, ids: Ids, entity: Entity) -> Entity {
        world
            .get(ids.member, entity)
            .map_or(entity, |member| member.horde)
    }

    fn horde_and_members(world: &World, ids: Ids, horde: Entity) -> Vec<Entity> {
        let mut all = vec![horde];
        if let Some(found) = world.get(ids.horde, horde) {
            all.extend(found.members.iter().copied());
        }
        all
    }

    fn bloodthirsty_module(
        &self,
        world: &World,
        ids: Ids,
        horde: Entity,
    ) -> Option<&mint_sage_modules::CapturedModule> {
        world
            .get(ids.kind, horde)
            .and_then(|kind| self.kinds.get(*kind))?
            .record
            .declared_modules
            .iter()
            .find(|module| module.name.eq_ignore_ascii_case(MODULE))
    }

    fn sacrifice_filter_passes(
        &self,
        world: &World,
        module: &mint_sage_modules::CapturedModule,
        sacrificer: Entity,
        victim: Entity,
    ) -> bool {
        let Some(field) = module.field("SacrificeFilter") else {
            return true;
        };
        field
            .args
            .iter()
            .any(|word| match word.to_ascii_uppercase().as_str() {
                "ALL" => true,
                "ALLIES" => self.object_allied(world, sacrificer, victim),
                "ENEMIES" => self.object_hostile(world, sacrificer, victim),
                "NEUTRAL" => {
                    !self.object_allied(world, sacrificer, victim)
                        && !self.object_hostile(world, sacrificer, victim)
                }
                _ => false,
            })
    }

    pub(super) fn start_sacrifice(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        target: Entity,
    ) -> Result<()> {
        let world = tick.state();
        let Some(lead) = self.selection(tick, ids, player).first().copied() else {
            return Ok(());
        };
        if world.get(ids.owner, lead).map(|owner| owner.0) != Some(player) {
            return Ok(());
        }
        let sacrificer = Self::sacrifice_home(world, ids, lead);
        let victim = Self::sacrifice_home(world, ids, target);
        let (Some(own_module), Some(_)) = (
            self.bloodthirsty_module(world, ids, sacrificer),
            self.bloodthirsty_module(world, ids, victim),
        ) else {
            return Ok(());
        };
        if sacrificer == victim
            || !world.contains(ids.horde, sacrificer)
            || !world.contains(ids.horde, victim)
            || world.contains(ids.sacrificed, victim)
            || world.contains(ids.sacrificing, victim)
            || Self::is_attacking(world, ids, victim)
            || world.contains(ids.path, victim)
            || !self.sacrifice_filter_passes(world, own_module, sacrificer, victim)
        {
            return Ok(());
        }
        let voice = own_module
            .field("InitiateVoice")
            .and_then(|field| field.args.first())
            .cloned();
        tick.state_mut().insert(
            ids.sacrificing,
            sacrificer,
            Sacrificing { victim, kills: 0 },
        )?;
        tick.state_mut()
            .insert(ids.sacrificed, victim, Sacrificed { by: sacrificer })?;
        let world = tick.state();
        let mut effects: Vec<(Entity, Effect)> = Self::horde_and_members(world, ids, sacrificer)
            .into_iter()
            .map(|entity| {
                (
                    entity,
                    Effect::StatusBits {
                        set: names(&["BLOODTHIRSTY"]),
                        clear: names(&[]),
                    },
                )
            })
            .collect();
        if let Some(voice) = voice {
            effects.push((lead, Effect::PlayObjectSound(voice.into())));
        }
        let held = Self::horde_and_members(world, ids, victim);
        self.apply_effects(tick, ids, effects)?;
        for entity in held {
            tick.state_mut().insert(
                ids.ignore_ai_command,
                entity,
                super::busy::IgnoreAiCommand(Frame::FOREVER),
            )?;
        }
        let world = tick.state();
        let others: Vec<Entity> = self
            .selection(tick, ids, player)
            .into_iter()
            .filter(|entity| Self::sacrifice_home(world, ids, *entity) != sacrificer)
            .collect();
        let saved = Self::deselect(tick, ids, player, &others)?;
        let result = self.script_attack_object(tick, ids, player, target);
        Self::restore_selection(tick, ids, &saved)?;
        result
    }

    pub(super) fn sacrifice_blow(
        &self,
        world: &World,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) -> bool {
        let home = Self::sacrifice_home(world, ids, attacker);
        world.get(ids.sacrificing, home).is_some_and(|held| {
            held.victim == Self::sacrifice_home(world, ids, victim)
                && self.has_status_bit(world, attacker, "BLOODTHIRSTY")
        })
    }

    pub(super) fn note_sacrifice_kill(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) -> Result<()> {
        let home = Self::sacrifice_home(tick.state(), ids, attacker);
        let Some(mut held) = tick.state().get(ids.sacrificing, home).copied() else {
            return Ok(());
        };
        held.kills = held.kills.saturating_add(1);
        tick.state_mut().insert(ids.sacrificing, home, held)?;
        if held.kills != 1 {
            return Ok(());
        }
        let afraid = Self::horde_and_members(
            tick.state(),
            ids,
            Self::sacrifice_home(tick.state(), ids, victim),
        )
        .into_iter()
        .map(|entity| {
            (
                entity,
                Effect::ModelConditions {
                    add: names(&["EMOTION_AFRAID"]),
                    remove: names(&[]),
                },
            )
        })
        .collect();
        self.apply_effects(tick, ids, afraid)
    }

    pub(super) fn end_sacrifice(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        sacrificer: Entity,
    ) -> Result<()> {
        let Some(held) = tick.state_mut().remove(ids.sacrificing, sacrificer) else {
            return Ok(());
        };
        tick.state_mut().remove(ids.sacrificed, held.victim);
        let world = tick.state();
        let mut effects: Vec<(Entity, Effect)> = Self::horde_and_members(world, ids, sacrificer)
            .into_iter()
            .map(|entity| {
                (
                    entity,
                    Effect::StatusBits {
                        set: names(&[]),
                        clear: names(&["BLOODTHIRSTY"]),
                    },
                )
            })
            .collect();
        let freed = Self::horde_and_members(world, ids, held.victim);
        effects.extend(freed.iter().map(|entity| {
            (
                *entity,
                Effect::ModelConditions {
                    add: names(&[]),
                    remove: names(&["EMOTION_AFRAID"]),
                },
            )
        }));
        self.apply_effects(tick, ids, effects)?;
        for entity in freed {
            if tick
                .state()
                .get(ids.ignore_ai_command, entity)
                .is_some_and(|hold| hold.0 == Frame::FOREVER)
            {
                tick.state_mut().remove(ids.ignore_ai_command, entity);
            }
            tick.state_mut().remove(ids.path, entity);
            tick.state_mut().remove(ids.attack_order, entity);
        }
        Ok(())
    }

    pub(super) fn end_sacrifice_for_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        commanded: &[Entity],
        order: &busy::HeldOrder,
    ) -> Result<()> {
        let mut ending: Vec<Entity> = Vec::new();
        for entity in commanded {
            let home = Self::sacrifice_home(tick.state(), ids, *entity);
            let Some(held) = tick.state().get(ids.sacrificing, home).copied() else {
                continue;
            };
            let at_victim = match order {
                busy::HeldOrder::AttackObject(target)
                | busy::HeldOrder::ForceAttackObject(target)
                | busy::HeldOrder::ScriptAttackObject(target) => {
                    Self::sacrifice_home(tick.state(), ids, *target) == held.victim
                }
                _ => false,
            };
            if !at_victim && !ending.contains(&home) {
                ending.push(home);
            }
        }
        for sacrificer in ending {
            self.end_sacrifice(tick, ids, sacrificer)?;
        }
        Ok(())
    }

    pub(super) fn step_sacrifices(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let world = tick.state();
        let living = |horde: Entity| {
            world.get(ids.horde, horde).is_some_and(|found| {
                found.members.iter().any(|member| {
                    world.is_live(*member)
                        && !world.contains(ids.doomed, *member)
                        && world
                            .get(ids.health, *member)
                            .is_some_and(|health| !health.0.is_dead())
                })
            })
        };
        let ended: Vec<Entity> = world
            .store(ids.sacrificing)
            .map(|store| {
                store
                    .iter()
                    .filter(|(sacrificer, held)| !living(*sacrificer) || !living(held.victim))
                    .map(|(sacrificer, _)| sacrificer)
                    .collect()
            })
            .unwrap_or_default();
        for sacrificer in ended {
            self.end_sacrifice(tick, ids, sacrificer)?;
        }
        Ok(())
    }
}
