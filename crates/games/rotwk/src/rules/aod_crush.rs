use super::*;
use crate::combat::{DamageEvent, DeathNaming, Foe, TargetGrid};
use crate::fx::FxSource;
use mint_sage_modules::object_filter::ObjectFilter;
use mint_sage_modules::{DeadCollideSize, Effect};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LargeUnitTouch {
    pub(crate) target: Entity,
    pub(crate) since: Frame,
    pub(crate) position: Vec2,
    pub(crate) height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct AodCrusher {
    pub(crate) firing_until: Option<Frame>,
    pub(crate) large_unit: Option<LargeUnitTouch>,
}

impl Component for AodCrusher {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.firing_until.is_some());
        hasher.write_u64(self.firing_until.map_or(0, Frame::get));
        hasher.write_bool(self.large_unit.is_some());
        if let Some(touch) = self.large_unit {
            hasher.write_u32(touch.target.index());
            hasher.write_u64(touch.since.get());
            hasher.write_f32(touch.position.x);
            hasher.write_f32(touch.position.y);
            hasher.write_f32(touch.height);
        }
    }
}

const FIRING_HOLD_MS: u32 = 2_000;

impl RotwkRules {
    pub(super) fn run_aod_crush_pass(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let crushers: Vec<Entity> = living
            .iter()
            .filter_map(|(foe, _)| {
                self.record_of(tick, ids, foe.entity)
                    .is_some_and(|record| record.aod_crush.is_some())
                    .then_some(foe.entity)
            })
            .collect();
        if crushers.is_empty() {
            return Ok(());
        }
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for crusher in crushers {
            self.run_one_aod_crusher(tick, ids, crusher, events, living, grid, &mut effects)?;
        }
        self.apply_effects(tick, ids, effects)
    }

    #[allow(clippy::too_many_arguments)]
    fn run_one_aod_crusher(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        crusher: Entity,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        if tick
            .state()
            .get(ids.health, crusher)
            .is_some_and(|health| health.0.is_dead())
        {
            return Ok(());
        }
        if self.holds_model_condition(tick, crusher, "OATH_FULLFILLED") {
            return Ok(());
        }
        let Some(crush) = self
            .record_of(tick, ids, crusher)
            .and_then(|record| record.aod_crush.clone())
        else {
            return Ok(());
        };
        let Some((_, position, radius)) = self.aod_crusher_place(tick, ids, crusher) else {
            return Ok(());
        };
        let touched = self.aod_overlaps(tick.state(), crusher, position, radius, living, grid);
        let now = tick.frame();
        let mut large_unit = tick
            .state()
            .get(ids.aod_crusher, crusher)
            .and_then(|crusher| crusher.large_unit);
        let mut hit_any = false;
        for (other, other_position) in touched {
            let other_record = self.record_of(tick, ids, other).cloned();
            let size = other_record
                .as_ref()
                .map_or(DeadCollideSize::default(), |record| {
                    record.dead_collide_size
                });
            if size == DeadCollideSize::Large
                && large_unit.is_none_or(|touch| touch.target != other)
            {
                let height = other_record
                    .as_ref()
                    .and_then(|record| record.geometry.height)
                    .unwrap_or(0.0);
                large_unit = Some(LargeUnitTouch {
                    target: other,
                    since: now,
                    position: other_position,
                    height,
                });
            }
            if !other_record.as_ref().is_some_and(has_physics) {
                continue;
            }
            hit_any = true;
            let special = other_record.as_ref().is_some_and(|record| {
                !crush.special_object.is_empty()
                    && ObjectFilter::parse(&crush.special_object)
                        .passes(&record.kind_of, &record.name)
            });
            let (amount, damage_name, death_name) = if special {
                (
                    crush.special_damage,
                    crush.special_damage_type.as_deref(),
                    crush.special_death_type.as_deref(),
                )
            } else {
                (
                    crush.damage,
                    crush.damage_type.as_deref(),
                    crush.death_type.as_deref(),
                )
            };
            self.push_aod_damage(events, other, crusher, amount, damage_name, death_name);
            if special {
                self.push_aod_damage(
                    events,
                    crusher,
                    other,
                    crush.self_damage,
                    crush.self_damage_type.as_deref(),
                    crush.self_death_type.as_deref(),
                );
            }
            let (fx, ocl) = match size {
                DeadCollideSize::Small => (&crush.small_fx, &crush.small_ocl),
                DeadCollideSize::Medium => (&crush.medium_fx, &crush.medium_ocl),
                DeadCollideSize::Large => (&crush.large_fx, &crush.large_ocl),
            };
            if let Some(list) = fx {
                self.emit_fx_object_named(
                    tick,
                    ids,
                    list,
                    other_position,
                    FxSource::Weapon,
                    other,
                    Some(crusher),
                );
            }
            if let Some(list) = ocl {
                if let Some(victim_owner) = tick.state().get(ids.owner, other).map(|o| o.0) {
                    self.run_creation_list(
                        tick,
                        ids,
                        other,
                        list,
                        other_position,
                        super::spells::CreationVariant::Object,
                        victim_owner,
                        Some(position),
                        None,
                    )?;
                }
            }
        }
        let stored = tick
            .state()
            .get(ids.aod_crusher, crusher)
            .copied()
            .unwrap_or_default();
        let mut next = AodCrusher {
            firing_until: stored.firing_until,
            large_unit,
        };
        if hit_any {
            if stored.firing_until.is_none() {
                effects.push((
                    crusher,
                    Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(vec!["FIRING_A".to_owned()]),
                        remove: mint_sage_modules::effects::names(Vec::new()),
                    },
                ));
            }
            next.firing_until = Some(
                now.checked_add(LogicFrames::from_engine_milliseconds(FIRING_HOLD_MS))
                    .unwrap_or(Frame::FOREVER),
            );
        } else if stored.firing_until.is_some_and(|until| now >= until) {
            effects.push((
                crusher,
                Effect::ModelConditions {
                    add: mint_sage_modules::effects::names(Vec::new()),
                    remove: mint_sage_modules::effects::names(vec!["FIRING_A".to_owned()]),
                },
            ));
            next.firing_until = None;
        }
        if next != stored {
            tick.state_mut().insert(ids.aod_crusher, crusher, next)?;
        }
        Ok(())
    }

    fn aod_crusher_place(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        crusher: Entity,
    ) -> Option<(PlayerId, Vec2, f32)> {
        let owner = tick.state().get(ids.owner, crusher)?.0;
        let body = tick.state().get(ids.body, crusher)?.0;
        let radius = self
            .record_of(tick, ids, crusher)
            .and_then(|record| record.geometry.radius)
            .unwrap_or(0.0);
        Some((owner, body.position, radius))
    }

    fn aod_overlaps(
        &self,
        world: &World,
        crusher: Entity,
        position: Vec2,
        radius: f32,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Vec<(Entity, Vec2)> {
        let (cx, cy) = (
            crate::combat::grid_cell(position.x),
            crate::combat::grid_cell(position.y),
        );
        let cells = crate::combat::grid_radius_cells(radius + 64.0);
        let mut near: Vec<u32> = Vec::new();
        grid.query_into(cx, cy, cells, &mut near);
        let mut touched = Vec::new();
        for index in near {
            let Some((foe, _)) = living.get(index as usize) else {
                continue;
            };
            if foe.entity == crusher {
                continue;
            }
            if self.object_allied(world, crusher, foe.entity) {
                continue;
            }
            let reach = radius + foe.radius;
            if foe.position.minus(position).length() > reach {
                continue;
            }
            touched.push((foe.entity, foe.position));
        }
        touched
    }

    fn push_aod_damage(
        &self,
        events: &mut Vec<DamageEvent>,
        victim: Entity,
        attacker: Entity,
        amount: f32,
        damage_type_name: Option<&str>,
        death_type_name: Option<&str>,
    ) {
        let Some(damage_type_name) = damage_type_name else {
            return;
        };
        let Some(damage_type) = self.combat.damage_types.lookup(damage_type_name) else {
            return;
        };
        let death =
            DeathNaming::of(death_type_name.and_then(|name| self.combat.death_types.lookup(name)));
        events.push(DamageEvent {
            victim,
            attacker,
            amount,
            damage_type,
            scalar: 1.0,
            fx_type: None,
            dealt: 0.0,
            death,
            sub_type: None,
            flanking_bonus: 0.0,
            weapon: None,
            launched: None,
            nugget: 0,
            accepts_add: true,
            bonuses_applied: false,
            force_kill: false,
            drain_life: 0.0,
        });
    }
}

fn has_physics(record: &mint_sage_modules::ObjectRecord) -> bool {
    record
        .declared_modules
        .iter()
        .any(|module| module.name.eq_ignore_ascii_case("PhysicsBehavior"))
}
