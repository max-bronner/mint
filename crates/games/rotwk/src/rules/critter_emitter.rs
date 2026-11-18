use super::*;
use crate::combat::Foe;
use mint_sage_asset::KindOf;
use mint_sage_modules::{CreateLocation, Effect, OclFrom, PowerPlace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CritterEmitterSpent {
    pub(crate) until: Frame,
}

impl Component for CritterEmitterSpent {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.until.hash_into(hasher);
    }
}

impl RotwkRules {
    pub(super) fn step_critter_emitters(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        if tick.frame().get() % u64::from(super::emotions::TICKS_PER_FRAME.max(1)) != 0 {
            return Ok(());
        }
        let emitters: Vec<(Entity, mint_sage_modules::CritterEmitter)> = living
            .iter()
            .filter_map(|(foe, _)| {
                let emitter = self
                    .record_of(tick, ids, foe.entity)?
                    .critter_emitter
                    .clone()?;
                Some((foe.entity, emitter))
            })
            .collect();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for (emitter, data) in emitters {
            let now = tick.frame();
            if let Some(spent) = tick.state().get(ids.critter_emitter_spent, emitter) {
                if now < spent.until {
                    continue;
                }
                tick.state_mut().remove(ids.critter_emitter_spent, emitter);
            }
            if !self.critter_emitter_touched(tick, ids, emitter, living) {
                continue;
            }
            if !data.fx.is_empty() {
                let pick = tick
                    .rng()
                    .below(u32::try_from(data.fx.len()).unwrap_or(u32::MAX));
                if let Some(list) = data.fx.get(pick as usize).filter(|n| !is_none(n)) {
                    effects.push((
                        emitter,
                        Effect::PlayFx {
                            list: list.as_str().into(),
                            at: PowerPlace::Source,
                            on_object: true,
                            secondary: None,
                        },
                    ));
                }
            }
            if !data.spawn_objects.is_empty() {
                let bound = u32::try_from(data.spawn_objects.len()).unwrap_or(u32::MAX);
                let pick = tick.rng().below(bound);
                if let Some(list) = data
                    .spawn_objects
                    .get(pick as usize)
                    .filter(|n| !is_none(n))
                {
                    effects.push((
                        emitter,
                        Effect::CreateObjects {
                            list: list.as_str().into(),
                            location: CreateLocation::AtLocation,
                            at: PowerPlace::Source,
                            from: OclFrom::Object { secondary: None },
                        },
                    ));
                }
            }
            let reload = mint_sage_modules::retail_duration_ticks(data.reload_ms);
            let until = now
                .checked_add(reload)
                .and_then(|at| at.checked_add(super::emotions::ONE_FRAME))
                .unwrap_or(Frame::FOREVER);
            tick.state_mut().insert(
                ids.critter_emitter_spent,
                emitter,
                CritterEmitterSpent { until },
            )?;
            diag!(
                "critter: frame {} emitter {} fires, spent until frame {}",
                now.get(),
                emitter.index(),
                until.get()
            );
        }
        self.apply_effects(tick, ids, effects)
    }

    fn critter_emitter_touched(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        emitter: Entity,
        living: &[(Foe, PlayerId)],
    ) -> bool {
        let world = tick.state();
        let (Some(body), Some(shape)) =
            (world.get(ids.body, emitter), world.get(ids.shape, emitter))
        else {
            return false;
        };
        if shape.kind_of.contains(KindOf::NO_COLLIDE) {
            return false;
        }
        let at = body.0.position;
        let z = world.get(ids.elevation, emitter).map(|e| e.0);
        living.iter().any(|(foe, _)| {
            if foe.entity == emitter || !world.contains(ids.path, foe.entity) {
                return false;
            }
            let Some(other) = world.get(ids.shape, foe.entity) else {
                return false;
            };
            if other.kind_of.contains(KindOf::NO_COLLIDE)
                || world
                    .get(ids.health, foe.entity)
                    .is_some_and(|health| health.0.is_dead())
            {
                return false;
            }
            let reach = shape.radius + other.radius;
            let (dx, dy) = (foe.position.x - at.x, foe.position.y - at.y);
            if dx * dx + dy * dy > reach * reach {
                return false;
            }
            z.is_none_or(|z| foe.z <= z + shape.height && foe.z + other.height >= z)
        })
    }
}

fn is_none(name: &str) -> bool {
    name.eq_ignore_ascii_case("None")
}
