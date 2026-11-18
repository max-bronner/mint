use mint_sage_ini::Stance;
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::diag::diag;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CombatStance(pub(crate) Stance);

impl Component for CombatStance {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.0.index());
    }
}

impl RotwkRules {
    pub(super) fn commanded_unit(world: &World, ids: Ids, entity: Entity) -> Entity {
        world
            .get(ids.member, entity)
            .map_or(entity, |member| member.horde)
    }

    fn stance_template_of<'a>(
        &'a self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<&'a str> {
        let kind = world.get(ids.kind, entity).copied()?;
        self.kinds.get(kind)?.record.stance_template.as_deref()
    }

    pub(crate) fn holds_a_stance(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        self.stance_template_of(world, ids, entity).is_some()
    }

    pub(crate) fn stance_of(&self, world: &World, ids: Ids, entity: Entity) -> Option<Stance> {
        let commanded = Self::commanded_unit(world, ids, entity);
        if !self.holds_a_stance(world, ids, commanded) {
            return None;
        }
        Some(
            world
                .get(ids.stance, commanded)
                .map_or(Stance::Battle, |held| held.0),
        )
    }

    pub(super) fn stance_list_of(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<mint_sage_combat::ModifierId> {
        let commanded = Self::commanded_unit(world, ids, entity);
        let template = self.stance_template_of(world, ids, commanded)?;
        let stance = self.stance_of(world, ids, commanded)?;
        let name = self
            .stances
            .get(template)?
            .stance(stance)?
            .attribute_modifier
            .as_deref()?;
        self.combat.modifiers.id(name)
    }

    pub(super) fn stance_melee_behavior(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<&mint_sage_ini::MeleeBehaviorDef> {
        let commanded = Self::commanded_unit(world, ids, entity);
        let template = self.stance_template_of(world, ids, commanded)?;
        let stance = self.stance_of(world, ids, commanded)?;
        self.stances
            .get(template)?
            .stance(stance)?
            .melee_behavior
            .as_ref()
    }

    pub(super) const TOGGLE_WEAPON_SET_FLAG: &'static str = "WEAPONSET_TOGGLE_1";

    fn weapon_set_toggle_raised(&self, tick: &Tick<'_>, entity: Entity) -> bool {
        self.modules
            .as_ref()
            .and_then(|wiring| tick.state().get(wiring.state, entity))
            .is_some_and(|state| state.has_weapon_set_flag(Self::TOGGLE_WEAPON_SET_FLAG))
    }

    pub(super) fn toggle_weapon_set(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        button: &str,
        lead: Option<Entity>,
    ) -> Result<()> {
        let mut units: Vec<Entity> = self
            .selection(tick, ids, player)
            .into_iter()
            .filter(|entity| {
                tick.state()
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == player)
            })
            .map(|entity| Self::commanded_unit(tick.state(), ids, entity))
            .collect();
        units.sort_unstable_by_key(|entity| entity.index());
        units.dedup();
        let lead = lead.map(|lead| Self::commanded_unit(tick.state(), ids, lead));
        let target = lead.map(|lead| !self.weapon_set_toggle_raised(tick, lead));
        for unit in units {
            if lead.is_some_and(|lead| {
                !self.weapon_set_toggle_equivalent(tick.state(), ids, unit, lead)
            }) {
                continue;
            }
            if self.occupied_non_horde_container(tick.state(), ids, unit) {
                continue;
            }
            self.press_weapon_toggle(tick, ids, unit, button, target)?;
            self.drop_attack_for_toggle(tick, ids, unit)?;
        }
        Ok(())
    }

    fn weapon_set_toggle_equivalent(&self, world: &World, ids: Ids, a: Entity, b: Entity) -> bool {
        let (Some(a), Some(b)) = (world.get(ids.kind, a), world.get(ids.kind, b)) else {
            return false;
        };
        if a == b {
            return true;
        }
        [a, b].into_iter().all(|kind| {
            self.kinds
                .get(*kind)
                .is_some_and(|entry| entry.record.has_kind("HOBBIT"))
        })
    }

    fn occupied_non_horde_container(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        !world.contains(ids.horde, entity)
            && world
                .get(ids.passengers, entity)
                .is_some_and(|list| !list.0.is_empty())
    }

    fn drop_attack_for_toggle(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        if !self.is_attacking_self_or_horde(tick, ids, entity) {
            return Ok(());
        }
        tick.state_mut().remove(ids.attack_order, entity);
        tick.state_mut().remove(ids.horde_target, entity);
        let until = tick
            .frame()
            .checked_add(LogicFrames::from_frames(5))
            .unwrap_or(Frame::FOREVER);
        tick.state_mut().insert(
            ids.ignore_ai_command,
            entity,
            super::busy::IgnoreAiCommand(until),
        )?;
        diag!(
            "command: entity {} drops its attack and ignores orders until frame {}",
            entity.index(),
            until.get()
        );
        Ok(())
    }

    pub(super) fn press_weapon_toggle(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        button: &str,
        target: Option<bool>,
    ) -> Result<bool> {
        let offers_toggle = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| {
                entry
                    .weapon_toggles
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(button))
            });
        if !offers_toggle {
            return Ok(false);
        }
        let raised = self.weapon_set_toggle_raised(tick, entity);
        if target == Some(raised) {
            return Ok(false);
        }
        let effect = if raised {
            mint_sage_modules::Effect::ClearWeaponSetFlag(mint_sage_modules::effects::Name::from(
                Self::TOGGLE_WEAPON_SET_FLAG,
            ))
        } else {
            mint_sage_modules::Effect::WeaponSetFlag(mint_sage_modules::effects::Name::from(
                Self::TOGGLE_WEAPON_SET_FLAG,
            ))
        };
        let targets = self.horde_members(tick, ids, &[entity]);
        let mut entries: Vec<(Entity, mint_sage_modules::Effect)> = targets
            .into_iter()
            .map(|target| (target, effect.clone()))
            .collect();
        if tick.state().contains(ids.horde, entity) {
            entries.push((entity, effect));
        }
        self.apply_effects(tick, ids, entries)?;
        diag!(
            "command: entity {} {} {button}",
            entity.index(),
            if raised { "drops" } else { "raises" }
        );
        Ok(true)
    }

    pub(super) fn set_stance(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        stance: Stance,
    ) -> Result<()> {
        let selected = self.selection(tick, ids, player);
        let mut units: Vec<Entity> = Vec::new();
        for entity in selected {
            let owned = tick
                .state()
                .get(ids.owner, entity)
                .is_some_and(|owner| owner.0 == player);
            if !owned {
                continue;
            }
            let commanded = Self::commanded_unit(tick.state(), ids, entity);
            if self.holds_a_stance(tick.state(), ids, commanded) {
                units.push(commanded);
            }
        }
        units.sort_unstable_by_key(|entity| entity.index());
        units.dedup();
        for unit in units {
            if !self.porcupine_gate(tick, ids, unit, false)? {
                continue;
            }
            self.apply_stance_change(tick, ids, unit, stance)?;
        }
        Ok(())
    }

    pub(super) fn apply_stance_change(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        stance: Stance,
    ) -> Result<()> {
        let previous = self.stance_of(tick.state(), ids, unit);
        if previous == Some(stance) {
            return Ok(());
        }
        let now = tick.frame();
        self.change_stance(tick, ids, unit, stance)?;
        let mode = match stance {
            Stance::Battle => Some(super::guard::GuardMode::Normal),
            Stance::HoldGround => {
                self.stop_unit(tick, ids, unit);
                Some(super::guard::GuardMode::HoldGround)
            }
            Stance::Porcupine => Some(super::guard::GuardMode::HoldGround),
            Stance::Aggressive | Stance::HoldGroundMoving => {
                if matches!(previous, Some(Stance::Battle | Stance::HoldGround)) {
                    self.stop_unit(tick, ids, unit);
                    self.leave_guard(tick, ids, unit);
                }
                None
            }
            Stance::Uninitialized => None,
        };
        let idle = !Self::guard_displaced(tick.state(), ids, unit);
        if let Some(mode) = mode.filter(|_| idle) {
            self.leave_guard(tick, ids, unit);
            if let Some(guard) = self.guard_here(tick, ids, unit, mode, now) {
                tick.state_mut().insert(ids.guard, unit, guard)?;
            }
        }
        if crate::diag::wants("stance") {
            diag!(
                "stance: entity {} to {} - guard radius {:.0}",
                unit.index(),
                stance.as_str(),
                self.guard_radius(tick, ids, unit),
            );
        }
        Ok(())
    }

    pub(super) fn change_stance(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        stance: Stance,
    ) -> Result<()> {
        tick.state_mut()
            .insert(ids.stance, unit, CombatStance(stance))?;
        let mut touched = vec![unit];
        if let Some(horde) = tick.state().get(ids.horde, unit) {
            touched.extend(horde.members.iter().copied());
        }
        for entity in touched {
            if !tick.state().is_live(entity) {
                continue;
            }
            self.recompute_bonuses(tick, ids, entity)?;
        }
        Ok(())
    }

    pub(super) fn stance_on_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
    ) -> Result<()> {
        if !self.holds_a_stance(tick.state(), ids, unit) {
            return Ok(());
        }
        let next = match self.stance_of(tick.state(), ids, unit) {
            Some(Stance::HoldGround) => Stance::HoldGroundMoving,
            _ => return Ok(()),
        };
        self.change_stance(tick, ids, unit, next)
    }

    pub(super) fn stop_unit(&self, tick: &mut Tick<'_>, ids: Ids, unit: Entity) {
        tick.state_mut().remove(ids.attack_order, unit);
        tick.state_mut().remove(ids.attack_move_order, unit);
        tick.state_mut().remove(ids.ground_order, unit);
        tick.state_mut().remove(ids.horde_target, unit);
        tick.state_mut().remove(ids.suspended_order, unit);
        tick.state_mut().remove(ids.order_queue, unit);
        if tick
            .state()
            .get(ids.path, unit)
            .is_some_and(|path| path.kind != crate::unit::PathKind::Ability)
        {
            tick.state_mut().remove(ids.path, unit);
            tick.state_mut().remove(ids.formation_speed, unit);
        }
    }

    pub(crate) fn stance_report(&self, world: &World, ids: Ids, player: PlayerId) -> Vec<String> {
        let armour = |entity: Entity| {
            world
                .get(ids.bonuses, entity)
                .map_or(0.0, |bonuses| bonuses.0.armor)
        };
        let mut units: Vec<Entity> = world
            .store(ids.selected)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, mask)| mask.holds(player))
                    .map(|(entity, _)| Self::commanded_unit(world, ids, entity))
                    .filter(|entity| self.holds_a_stance(world, ids, *entity))
                    .collect()
            })
            .unwrap_or_default();
        units.sort_unstable_by_key(|entity| entity.index());
        units.dedup();
        units
            .into_iter()
            .map(|unit| {
                let members: Vec<Entity> = world
                    .get(ids.horde, unit)
                    .map(|horde| {
                        horde
                            .members
                            .iter()
                            .copied()
                            .filter(|member| world.is_live(*member))
                            .collect()
                    })
                    .unwrap_or_default();
                let total: f32 = members.iter().map(|member| armour(*member)).sum();
                format!(
                    "entity {} template {} stance {} armour {:+.2}, {} member(s) armour {:+.2}",
                    unit.index(),
                    self.stance_template_of(world, ids, unit).unwrap_or("?"),
                    self.stance_of(world, ids, unit)
                        .map_or("-", mint_sage_ini::Stance::as_str),
                    armour(unit),
                    members.len(),
                    total,
                )
            })
            .collect()
    }

    pub(crate) fn next_stance(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        cycle: &[Stance],
    ) -> Option<Stance> {
        let mut units: Vec<Entity> = world
            .store(ids.selected)?
            .iter()
            .filter(|(_, mask)| mask.holds(player))
            .map(|(entity, _)| Self::commanded_unit(world, ids, entity))
            .filter(|entity| self.holds_a_stance(world, ids, *entity))
            .collect();
        units.sort_unstable_by_key(|entity| entity.index());
        let current = self.stance_of(world, ids, *units.first()?)?;
        let at = cycle.iter().position(|stance| *stance == current);
        let next = at.map_or(0, |at| at.saturating_add(1) % cycle.len());
        cycle.get(next).copied()
    }
}
