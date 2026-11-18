use mint_sage_combat::DamageTypeId;
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick};

use super::{Ids, RotwkRules};
use crate::combat::{DeathNaming, DotHit, Foe, TargetGrid};
use crate::diag::diag;
use crate::templates::KindTable;
use crate::unit::Kind;

#[derive(Debug, Clone, PartialEq)]
struct FlammableSpec {
    damage_type: Option<DamageTypeId>,
    flame_damage_limit: f32,
    flame_damage_expiration: LogicFrames,
    aflame_duration: Option<LogicFrames>,
    burned_delay: LogicFrames,
    set_burned_status: bool,
    aflame_damage_delay: LogicFrames,
    aflame_damage_amount: f32,
    swap_model_when_aflame: bool,
    swap_model_when_quenched: bool,
    swap_texture_when_aflame: bool,
    swap_texture_when_quenched: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SpreadSpec {
    try_range: f32,
    min_delay: LogicFrames,
    max_delay: LogicFrames,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FlammableSpecs {
    flammable: Vec<Option<FlammableSpec>>,
    spread: Vec<Option<SpreadSpec>>,
    report: (usize, usize),
}

impl FlammableSpecs {
    pub(crate) fn build(kinds: &KindTable, damage_types: &mint_sage_combat::DamageTypes) -> Self {
        let mut out = Self::default();
        for index in 0..kinds.len() {
            let entry = u16::try_from(index).ok().and_then(|k| kinds.get(Kind(k)));
            let flammable = entry.and_then(|entry| {
                let module = entry
                    .record
                    .declared_modules
                    .iter()
                    .rev()
                    .find(|module| module.name.eq_ignore_ascii_case("FlammableUpdate"))?;
                let bits = mint_sage_modules::UpgradeBits::default();
                let flammable =
                    mint_sage_modules::flammable::flammable_from_fields(&module.source(&bits))
                        .ok()?;
                out.report.0 = out.report.0.saturating_add(1);
                Some(FlammableSpec {
                    damage_type: flammable
                        .damage_type
                        .as_deref()
                        .and_then(|name| damage_types.lookup(name)),
                    flame_damage_limit: flammable.flame_damage_limit,
                    flame_damage_expiration: flammable.flame_damage_expiration,
                    aflame_duration: flammable.aflame_duration,
                    burned_delay: flammable.burned_delay,
                    set_burned_status: flammable.set_burned_status,
                    aflame_damage_delay: flammable.aflame_damage_delay,
                    aflame_damage_amount: flammable.aflame_damage_amount,
                    swap_model_when_aflame: flammable.swap_model_when_aflame,
                    swap_model_when_quenched: flammable.swap_model_when_quenched,
                    swap_texture_when_aflame: flammable.swap_texture_when_aflame,
                    swap_texture_when_quenched: flammable.swap_texture_when_quenched,
                })
            });
            let spread = entry.and_then(|entry| {
                let module = entry
                    .record
                    .declared_modules
                    .iter()
                    .rev()
                    .find(|module| module.name.eq_ignore_ascii_case("FireSpreadUpdate"))?;
                let spread = mint_sage_modules::flammable::fire_spread_from_fields(
                    &module.source(&mint_sage_modules::UpgradeBits::default()),
                );
                out.report.1 = out.report.1.saturating_add(1);
                Some(SpreadSpec {
                    try_range: spread.try_range,
                    min_delay: spread.min_spread_delay,
                    max_delay: spread.max_spread_delay,
                })
            });
            out.flammable.push(flammable);
            out.spread.push(spread);
        }
        out
    }

    fn get(&self, kind: Kind) -> Option<&FlammableSpec> {
        self.flammable.get(usize::from(kind.0))?.as_ref()
    }

    fn spread_of(&self, kind: Kind) -> Option<SpreadSpec> {
        *self.spread.get(usize::from(kind.0))?
    }

    pub(crate) fn describe(&self) -> String {
        format!(
            "flammable: {} kind(s) can catch fire ({} of them can spread it)",
            self.report.0, self.report.1
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Aflame {
    ends_at: Frame,
    burned_at: Frame,
    burned: bool,
    next_spread: Frame,
}

impl Component for Aflame {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.ends_at.hash_into(hasher);
        self.burned_at.hash_into(hasher);
        hasher.write_bool(self.burned);
        self.next_spread.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FlameHeat {
    remaining: f32,
    resets_at: Frame,
}

impl Component for FlameHeat {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.remaining);
        self.resets_at.hash_into(hasher);
    }
}

impl RotwkRules {
    pub(super) fn count_flame_hit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
        damage_type: DamageTypeId,
        dealt: f32,
    ) -> Result<()> {
        if dealt <= 0.0 {
            return Ok(());
        }
        let Some(kind) = tick.state().get(ids.kind, victim).copied() else {
            return Ok(());
        };
        let Some(spec) = self.flammable_specs.get(kind).cloned() else {
            return Ok(());
        };
        if spec
            .damage_type
            .is_some_and(|required| required != damage_type)
        {
            return Ok(());
        }
        if self.holds_condition(tick, victim, "AFLAME")
            || self.holds_condition(tick, victim, "BURNED")
        {
            return Ok(());
        }
        let now = tick.frame();
        let mut heat = tick
            .state()
            .get(ids.flame_heat, victim)
            .copied()
            .unwrap_or(FlameHeat {
                remaining: spec.flame_damage_limit,
                resets_at: Frame::FOREVER,
            });
        if now > heat.resets_at {
            heat.remaining = spec.flame_damage_limit;
        }
        heat.remaining -= dealt;
        heat.resets_at = now
            .checked_add(spec.flame_damage_expiration)
            .unwrap_or(Frame::FOREVER);
        if heat.remaining <= 0.0 {
            tick.state_mut().remove(ids.flame_heat, victim);
            self.ignite(tick, ids, victim, &spec, now)?;
        } else {
            tick.state_mut().insert(ids.flame_heat, victim, heat)?;
        }
        Ok(())
    }

    fn ignite(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        spec: &FlammableSpec,
        now: Frame,
    ) -> Result<()> {
        let ends_at = spec
            .aflame_duration
            .and_then(|duration| now.checked_add(duration))
            .unwrap_or(Frame::FOREVER);
        let burned_at = now.checked_add(spec.burned_delay).unwrap_or(Frame::FOREVER);
        tick.state_mut().insert(
            ids.aflame,
            entity,
            Aflame {
                ends_at,
                burned_at,
                burned: false,
                next_spread: now,
            },
        )?;
        let mut add = vec!["AFLAME".to_owned()];
        if spec.swap_model_when_aflame {
            add.push("BURNT_MODEL".to_owned());
        }
        if spec.swap_texture_when_aflame {
            add.push("BURNT_TEXTURE".to_owned());
        }
        self.apply_effects(
            tick,
            ids,
            vec![
                (
                    entity,
                    mint_sage_modules::effects::Effect::StatusBits {
                        set: mint_sage_modules::effects::names(vec!["AFLAME".to_owned()]),
                        clear: mint_sage_modules::effects::names(Vec::new()),
                    },
                ),
                (
                    entity,
                    mint_sage_modules::effects::Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(add),
                        remove: mint_sage_modules::effects::names(Vec::new()),
                    },
                ),
            ],
        )?;
        if let Some(damage_type) = self.combat.damage_types.lookup("FLAME") {
            if spec.aflame_damage_amount > 0.0 && !spec.aflame_damage_delay.is_zero() {
                let mut dots = tick
                    .state()
                    .get(ids.dots, entity)
                    .cloned()
                    .unwrap_or_default();
                dots.hit(
                    now,
                    &DotHit {
                        victim: entity,
                        attacker: entity,
                        amount: spec.aflame_damage_amount,
                        damage_type,
                        interval: spec.aflame_damage_delay,
                        duration: spec
                            .aflame_duration
                            .unwrap_or(LogicFrames::from_frames(u32::MAX)),
                        death: DeathNaming::Normal,
                        sub_type: None,
                        accepts_add: true,
                        from_nugget: false,
                    },
                );
                tick.state_mut().insert(ids.dots, entity, dots)?;
            }
        }
        self.fire_object_event(tick, ids, entity, "OnFire", None)?;
        diag!(
            "fire: entity {} ignites at frame {}, ends at frame {}",
            entity.index(),
            now.get(),
            ends_at.get()
        );
        Ok(())
    }

    fn extinguish(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        spec: &FlammableSpec,
    ) -> Result<()> {
        let mut remove = vec!["AFLAME".to_owned()];
        let mut add = Vec::new();
        if spec.swap_model_when_quenched {
            remove.push("BURNT_MODEL".to_owned());
        }
        if spec.swap_texture_when_quenched {
            remove.push("BURNT_TEXTURE".to_owned());
        }
        let mut status_set = Vec::new();
        if spec.set_burned_status {
            status_set.push("BURNED".to_owned());
            add.push("BURNED".to_owned());
        }
        self.apply_effects(
            tick,
            ids,
            vec![
                (
                    entity,
                    mint_sage_modules::effects::Effect::StatusBits {
                        set: mint_sage_modules::effects::names(status_set),
                        clear: mint_sage_modules::effects::names(vec!["AFLAME".to_owned()]),
                    },
                ),
                (
                    entity,
                    mint_sage_modules::effects::Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(add),
                        remove: mint_sage_modules::effects::names(remove),
                    },
                ),
            ],
        )?;
        tick.state_mut().remove(ids.aflame, entity);
        self.fire_object_event(tick, ids, entity, "OnFireOut", None)?;
        Ok(())
    }

    pub(super) fn tick_fire(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let now = tick.frame();
        let entities: Vec<Entity> = tick
            .state()
            .store(ids.aflame)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in entities {
            let Some(kind) = tick.state().get(ids.kind, entity).copied() else {
                continue;
            };
            let Some(spec) = self.flammable_specs.get(kind).cloned() else {
                continue;
            };
            let Some(mut aflame) = tick.state().get(ids.aflame, entity).copied() else {
                continue;
            };
            if !aflame.burned && spec.set_burned_status && now >= aflame.burned_at {
                aflame.burned = true;
                self.apply_effects(
                    tick,
                    ids,
                    vec![(
                        entity,
                        mint_sage_modules::effects::Effect::StatusBits {
                            set: mint_sage_modules::effects::names(vec!["BURNED".to_owned()]),
                            clear: mint_sage_modules::effects::names(Vec::new()),
                        },
                    )],
                )?;
                tick.state_mut().insert(ids.aflame, entity, aflame)?;
            }
            if now >= aflame.ends_at {
                self.extinguish(tick, ids, entity, &spec)?;
                continue;
            }
            let Some(spread) = self.flammable_specs.spread_of(kind) else {
                continue;
            };
            if now < aflame.next_spread {
                continue;
            }
            self.try_spread(tick, ids, entity, spread, living, grid)?;
            let span = spread
                .max_delay
                .get()
                .saturating_sub(spread.min_delay.get())
                .saturating_add(1);
            let roll = tick.rng().below(span);
            let delay =
                LogicFrames::from_frames(spread.min_delay.get().saturating_add(roll).max(1));
            aflame.next_spread = now.checked_add(delay).unwrap_or(Frame::FOREVER);
            tick.state_mut().insert(ids.aflame, entity, aflame)?;
        }
        Ok(())
    }

    fn try_spread(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        spread: SpreadSpec,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        if spread.try_range <= 0.0 {
            return Ok(());
        }
        let Some(position) = tick
            .state()
            .get(ids.body, entity)
            .map(|body| body.0.position)
        else {
            return Ok(());
        };
        let mut candidates: Vec<u32> = Vec::new();
        grid.query_into(
            crate::combat::grid_cell(position.x),
            crate::combat::grid_cell(position.y),
            crate::combat::grid_radius_cells(spread.try_range),
            &mut candidates,
        );
        let now = tick.frame();
        for index in candidates {
            let Some((foe, _)) = usize::try_from(index).ok().and_then(|at| living.get(at)) else {
                continue;
            };
            if foe.entity == entity {
                continue;
            }
            let dx = foe.position.x - position.x;
            let dy = foe.position.y - position.y;
            if dx * dx + dy * dy > spread.try_range * spread.try_range {
                continue;
            }
            let Some(kind) = tick.state().get(ids.kind, foe.entity).copied() else {
                continue;
            };
            let Some(target_spec) = self.flammable_specs.get(kind).cloned() else {
                continue;
            };
            if self.holds_condition(tick, foe.entity, "AFLAME")
                || self.holds_condition(tick, foe.entity, "BURNED")
            {
                continue;
            }
            self.ignite(tick, ids, foe.entity, &target_spec, now)?;
            return Ok(());
        }
        Ok(())
    }
}
