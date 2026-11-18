use mint_sim::{Entity, Frame, LogicFrames, PlayerId, Result, Tick, World};

use super::{Ids, RotwkRules};
use crate::construction::{AutoRepair, Repairing, WorkerStage};
use crate::diag::diag;

const WAKE: LogicFrames = LogicFrames::from_engine_milliseconds(1000);

const UNDER_ATTACK: LogicFrames = LogicFrames::from_engine_milliseconds(4000);

pub(super) fn hit_recently_at(world: &World, ids: Ids, entity: Entity, now: Frame) -> bool {
    world
        .get(ids.last_hurt, entity)
        .and_then(|hurt| hurt.0)
        .is_some_and(|hurt| now.get() <= hurt.get().saturating_add(u64::from(UNDER_ATTACK.get())))
}

pub(super) fn is_wake(frame: mint_sim::Frame) -> bool {
    frame.get() % u64::from(WAKE.get()) == 0
}

impl RotwkRules {
    pub(super) fn tend_auto_repair(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        self.heal_repairs(tick, ids)?;
        if is_wake(tick.frame()) {
            self.wake_repairs(tick, ids)?;
        }
        Ok(())
    }

    fn hit_recently(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        hit_recently_at(tick.state(), ids, entity, tick.frame())
    }

    fn heal_repairs(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let repairs: Vec<(Entity, AutoRepair)> = tick
            .state()
            .store(ids.auto_repair)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, state)| (entity, *state))
                    .collect()
            })
            .unwrap_or_default();
        for (structure, mut state) in repairs {
            let Some(mut health) = tick.state().get(ids.health, structure).copied() else {
                continue;
            };
            if health.0.is_dead() {
                continue;
            }
            let max = health.0.max();
            let heal = match state.repair {
                Repairing::Nothing => continue,
                Repairing::Itself { per_frame } => {
                    let interruptible = self
                        .record_of(tick, ids, structure)
                        .and_then(|record| record.spawn_timer)
                        .is_some_and(|timer| timer >= 0.0);
                    if interruptible && self.hit_recently(tick, ids, structure) {
                        state.repair = Repairing::Nothing;
                        tick.state_mut().insert(ids.auto_repair, structure, state)?;
                        continue;
                    }
                    per_frame
                }
                Repairing::Worker(worker) => {
                    let hammering = tick
                        .state()
                        .get(ids.site_worker, worker)
                        .is_some_and(|site| {
                            site.site == structure && site.stage == WorkerStage::Constructing
                        });
                    let rate = tick
                        .state()
                        .get(ids.kind, worker)
                        .and_then(|kind| self.kinds.get(*kind))
                        .and_then(|entry| entry.record.ai.repair_health_per_second)
                        .unwrap_or(0.0);
                    if hammering {
                        max * rate * LogicFrames::ONE.as_seconds()
                    } else {
                        0.0
                    }
                }
            };
            health.0.set_current(health.0.current() + heal);
            tick.state_mut().insert(ids.health, structure, health)?;
            if health.0.current() >= max {
                if let Repairing::Worker(worker) = state.repair {
                    self.release_site_worker(tick, ids, Some(worker))?;
                }
                tick.state_mut().remove(ids.auto_repair, structure);
                diag!(
                    "repair: frame {} entity {} repaired",
                    tick.frame().get(),
                    structure.index()
                );
            }
        }
        Ok(())
    }

    fn wake_repairs(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let candidates: Vec<Entity> = tick
            .state()
            .store(ids.health)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, health)| health.0.current() < health.0.max())
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for structure in candidates {
            let world = tick.state();
            if world.contains(ids.construction, structure) {
                continue;
            }
            let (Some(record), Some(owner)) = (
                self.record_of(tick, ids, structure),
                world.get(ids.owner, structure).map(|owner| owner.0),
            ) else {
                continue;
            };
            let Some(spawn_timer) = record.spawn_timer else {
                continue;
            };
            let rubble = world.contains(ids.doomed, structure);
            if rubble && !(record.rebuild_when_dead && self.is_rubble(world, ids, structure)) {
                continue;
            }
            if rubble {
                let at = world.get(ids.body, structure).map(|body| body.0.position);
                if at.is_some_and(|at| self.rebuild_disallowed(world, ids, record, structure, at)) {
                    continue;
                }
            }
            let mut state = world
                .get(ids.auto_repair, structure)
                .copied()
                .unwrap_or(AutoRepair {
                    timer: spawn_timer,
                    repair: Repairing::Nothing,
                });
            match state.repair {
                Repairing::Itself { .. } => continue,
                Repairing::Worker(worker)
                    if world.is_live(worker) && !world.contains(ids.doomed, worker) =>
                {
                    continue
                }
                Repairing::Worker(_) | Repairing::Nothing => state.repair = Repairing::Nothing,
            }
            let hit = self.hit_recently(tick, ids, structure);
            let start = if record.worker_name.is_some() {
                if state.timer > 0.0 && !hit {
                    state.timer -= 1.0;
                }
                state.timer <= 0.0
            } else if hit {
                false
            } else if record.use_spawn_timer_without_worker {
                if state.timer > 0.0 {
                    state.timer -= 1.0;
                }
                state.timer <= 0.0
            } else {
                state.timer >= 0.0
            };
            if start {
                state.repair = self.start_repair(tick, ids, structure, owner, rubble)?;
                if rubble {
                    tick.state_mut().remove(ids.auto_repair, structure);
                    continue;
                }
            }
            tick.state_mut().insert(ids.auto_repair, structure, state)?;
        }
        Ok(())
    }

    pub(super) fn start_repair(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        structure: Entity,
        owner: PlayerId,
        rubble: bool,
    ) -> Result<Repairing> {
        let Some(record) = self.record_of(tick, ids, structure).cloned() else {
            return Ok(Repairing::Nothing);
        };
        let frames = self.build_frames(&record, Some(owner));
        if rubble {
            let frames = record
                .rebuild_seconds
                .map(crate::game_data::rebuild_time_ticks)
                .or(frames)
                .unwrap_or(1);
            self.rebuild_rubble(tick, ids, structure, &record, owner, frames)?;
            diag!(
                "repair: frame {} entity {} rebuilds from rubble over {frames} frame(s)",
                tick.frame().get(),
                structure.index()
            );
            return Ok(Repairing::Nothing);
        }
        let repair = if record.worker_name.is_some() {
            let kind = self.getting_built_worker(&record, owner);
            match self.spawn_site_worker(tick, ids, kind, structure, owner)? {
                Some(worker) => Repairing::Worker(worker),
                None => Repairing::Nothing,
            }
        } else {
            let max = tick
                .state()
                .get(ids.health, structure)
                .map_or(0.0, |health| health.0.max());
            #[allow(clippy::cast_precision_loss)]
            frames.map_or(Repairing::Nothing, |frames| Repairing::Itself {
                per_frame: max / frames as f32,
            })
        };
        diag!(
            "repair: frame {} entity {} starts {repair:?}",
            tick.frame().get(),
            structure.index()
        );
        Ok(repair)
    }
}
