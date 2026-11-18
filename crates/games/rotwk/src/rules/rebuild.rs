use mint_sage_combat::HealthPool;
use mint_sage_modules::ObjectFilter;
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::construction::Construction;
use crate::diag::diag;
use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RebuildHole {
    pub(crate) structure: Kind,
    pub(crate) rebuild_at: Frame,
    pub(crate) reconstruction: Option<Entity>,
    pub(crate) regen_per_second: f32,
    pub(crate) faded_in_at: Frame,
}

impl Component for RebuildHole {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.structure.hash_into(hasher);
        self.rebuild_at.hash_into(hasher);
        hasher.write_bool(self.reconstruction.is_some());
        if let Some(site) = self.reconstruction {
            site.hash_into(hasher);
        }
        hasher.write_f32(self.regen_per_second);
        self.faded_in_at.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sold;

impl Component for Sold {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

pub(crate) const SOLD_STATUS: &str = "SOLD";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelfRepair {
    Hidden,
    Rebuild,
    Repair,
    Repairing,
    Refused(&'static str),
}

const MASK_STATUSES: [&str; 2] = ["UNSELECTABLE", "UNATTACKABLE"];

impl RotwkRules {
    pub(crate) fn is_rubble(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        let kept = world
            .get(ids.doomed, entity)
            .is_some_and(|doomed| doomed.destroy_at == Frame::FOREVER)
            && world
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
                .is_some_and(|entry| entry.record.keeps_corpse);
        kept && self
            .collapse_view(world, entity)
            .is_none_or(|(state, _, _)| state == mint_sage_modules::CollapseState::Done)
    }

    pub(crate) fn self_repair(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        structure: Entity,
        now: Frame,
    ) -> SelfRepair {
        let Some(record) = world
            .get(ids.kind, structure)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record)
        else {
            return SelfRepair::Hidden;
        };
        let Some(spawn_timer) = record.spawn_timer else {
            return SelfRepair::Hidden;
        };
        if world.contains(ids.construction, structure) {
            return SelfRepair::Hidden;
        }
        if world.get(ids.owner, structure).map(|owner| owner.0) != Some(player) {
            return SelfRepair::Refused("not the rebuilder's");
        }
        if world
            .get(ids.auto_repair, structure)
            .is_some_and(|state| state.repair != crate::construction::Repairing::Nothing)
        {
            return SelfRepair::Repairing;
        }
        let rubble = self.is_rubble(world, ids, structure);
        if spawn_timer >= 0.0 {
            if !rubble {
                return SelfRepair::Refused("not rubble");
            }
        } else {
            if super::repair::hit_recently_at(world, ids, structure, now) {
                return SelfRepair::Refused("under attack");
            }
            let hurt = world
                .get(ids.health, structure)
                .is_some_and(|health| health.0.current() < health.0.max());
            if !hurt {
                return SelfRepair::Refused("undamaged");
            }
        }
        let verdict = if !rubble {
            SelfRepair::Repair
        } else if record.rebuild_seconds.is_none() {
            return SelfRepair::Refused("no RebuildTimeSeconds");
        } else {
            SelfRepair::Rebuild
        };
        let purse = crate::economy::purse_of(world, Self::economy_ids(ids), player);
        if !purse.can_afford(self.self_repair_cost(world, ids, player, structure)) {
            return SelfRepair::Refused("no money");
        }
        verdict
    }

    pub(crate) fn self_repair_cost(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        structure: Entity,
    ) -> i64 {
        use crate::combat::{DamageRung, DamageThresholds};
        let Some(record) = world
            .get(ids.kind, structure)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record)
        else {
            return 0;
        };
        let price = self
            .price_for(world, ids, player, record)
            .map_or(0, |(price, _)| price);
        let rung = if self.is_rubble(world, ids, structure) {
            DamageRung::Rubble
        } else {
            let fraction = world.get(ids.health, structure).map_or(1.0, |health| {
                let max = health.0.max();
                if max > 0.0 {
                    health.0.current() / max
                } else {
                    1.0
                }
            });
            DamageThresholds::of(Some(record), self.damage_thresholds).rung(fraction)
        };
        let [pristine, damaged, really, rubble] = record.rebuild_cost_percent;
        let percent = match rung {
            DamageRung::Pristine => pristine,
            DamageRung::Damaged => damaged,
            DamageRung::ReallyDamaged => really,
            DamageRung::Rubble => rubble,
        };
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let cost = (price as f32 * percent).ceil() as i64;
        cost.max(0)
    }

    pub(super) fn charge_structure_cost(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        cost: i64,
    ) -> Result<bool> {
        let economy = Self::economy_ids(ids);
        let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, player)
        else {
            return Ok(false);
        };
        let mut purse = crate::economy::purse_of(tick.state(), economy, player);
        if !purse.can_afford(cost) {
            return Ok(false);
        }
        purse.withdraw(cost);
        tick.state_mut()
            .insert(ids.purse, player_entity, crate::economy::Purse(purse))?;
        crate::match_stats::record(tick, ids.match_stats, player_entity, |stats| {
            stats.spend(crate::match_stats::SpendKind::Structures, cost);
        });
        Ok(true)
    }

    pub(super) fn start_self_repair(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Result<()> {
        for structure in self.selection(tick, ids, player) {
            let verdict = self.self_repair(tick.state(), ids, player, structure, tick.frame());
            if matches!(verdict, SelfRepair::Rebuild | SelfRepair::Repair) {
                let cost = self.self_repair_cost(tick.state(), ids, player, structure);
                if !self.charge_structure_cost(tick, ids, player, cost)? {
                    continue;
                }
            }
            match verdict {
                SelfRepair::Rebuild => {}
                SelfRepair::Repair => {
                    let repair = self.start_repair(tick, ids, structure, player, false)?;
                    let timer = self
                        .record_of(tick, ids, structure)
                        .and_then(|record| record.spawn_timer)
                        .unwrap_or(0.0);
                    tick.state_mut().insert(
                        ids.auto_repair,
                        structure,
                        crate::construction::AutoRepair { timer, repair },
                    )?;
                    continue;
                }
                refused => {
                    diag!(
                        "rebuild: entity {} refused - {refused:?}",
                        structure.index()
                    );
                    continue;
                }
            }
            let Some(entry) = tick
                .state()
                .get(ids.kind, structure)
                .and_then(|kind| self.kinds.get(*kind))
                .cloned()
            else {
                continue;
            };
            let Some(seconds) = entry.record.rebuild_seconds else {
                continue;
            };
            let frames = LogicFrames::from_seconds(seconds).get().max(1);
            self.rebuild_rubble(tick, ids, structure, &entry.record, player, frames)?;
            diag!(
                "rebuild: frame {} entity {} rebuilding for player {} over {frames} frame(s)",
                tick.frame().get(),
                structure.index(),
                player.get()
            );
        }
        Ok(())
    }

    pub(super) fn rebuild_rubble(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        structure: Entity,
        record: &mint_sage_modules::ObjectRecord,
        owner: PlayerId,
        frames: u32,
    ) -> Result<()> {
        tick.state_mut().remove(ids.doomed, structure);
        super::map_scripts::note_object_count_changed(tick, ids);
        let footprint = tick
            .state()
            .get(ids.body, structure)
            .map(|body| body.0)
            .zip(
                tick.state()
                    .store(ids.obstacles)
                    .and_then(|store| store.entities().first().copied())
                    .and_then(|clock| tick.state().get(ids.obstacles, clock))
                    .and_then(|layer| layer.records_of(structure)),
            );
        if let Some((body, records)) = footprint {
            self.stamp_footprint(
                tick,
                ids,
                structure,
                body.position,
                body.facing.radians(),
                records,
            )?;
        }
        if let Some(mut health) = tick.state().get(ids.health, structure).copied() {
            if let Some(mut fresh) = HealthPool::new(health.0.max(), health.0.kind()) {
                fresh.set_current(1.0);
                health.0 = fresh;
                tick.state_mut().insert(ids.health, structure, health)?;
            }
        }
        let worker_kind = self.getting_built_worker(record, owner);
        let worker = self.spawn_site_worker(tick, ids, worker_kind, structure, owner)?;
        tick.state_mut().insert(
            ids.construction,
            structure,
            Construction {
                builder: None,
                elapsed: 0,
                rebuild: Some(frames),
                worker,
                delay: 0,
            },
        )?;
        Ok(())
    }
}

struct HoleExposure {
    hole: Kind,
    structure: Kind,
    died: Entity,
    at: mint_movement::Vec2,
    facing: mint_movement::Facing,
    owner: PlayerId,
    max_health: f32,
    fade_in: LogicFrames,
    transfer_attackers: bool,
}

impl RotwkRules {
    pub(super) fn expose_rebuild_holes(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        killed: &[Entity],
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let world = tick.state();
        let mut exposures: Vec<HoleExposure> = Vec::new();
        for &died in killed {
            let Some(module) = wiring.expose_die(world, died) else {
                continue;
            };
            if module.exempts(SOLD_STATUS) && world.contains(ids.sold, died) {
                continue;
            }
            if world.contains(ids.construction, died) {
                continue;
            }
            let (Some(name), Some(max_health)) = (module.hole_name(), module.hole_max_health())
            else {
                continue;
            };
            let Some(hole) = self.kinds.kind(name) else {
                diag!("rebuild: hole {name} is not in this match's templates");
                continue;
            };
            let (Some(structure), Some(body), Some(owner)) = (
                world.get(ids.kind, died).copied(),
                world.get(ids.body, died).map(|body| body.0),
                world.get(ids.owner, died).map(|owner| owner.0),
            ) else {
                continue;
            };
            if Some(owner) == self.neutral_player() {
                continue;
            }
            exposures.push(HoleExposure {
                hole,
                structure,
                died,
                at: body.position,
                facing: body.facing,
                owner,
                max_health,
                fade_in: module.fade_in().unwrap_or(LogicFrames::ZERO),
                transfer_attackers: module.transfer_attackers(),
            });
        }
        for exposure in exposures {
            self.expose_one_hole(tick, ids, &exposure)?;
        }
        Ok(())
    }

    fn expose_one_hole(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        exposure: &HoleExposure,
    ) -> Result<()> {
        let now = tick.frame();
        let Some(hole) = self.spawn_kind_at(
            tick,
            ids,
            exposure.hole,
            exposure.at,
            exposure.facing,
            exposure.owner,
        )?
        else {
            return Ok(());
        };
        self.take_over_geometry(tick, ids, exposure, hole)?;
        if let Some(mut health) = tick.state().get(ids.health, hole).copied() {
            if let Some(mut pool) = HealthPool::new(exposure.max_health, health.0.kind()) {
                pool.set_current(exposure.max_health);
                health.0 = pool;
                tick.state_mut().insert(ids.health, hole, health)?;
            }
        }
        let (delay, regen) = self
            .modules
            .as_ref()
            .and_then(|wiring| wiring.rebuild_hole(tick.state(), hole))
            .map_or((LogicFrames::ZERO, 0.0), |module| {
                (
                    module.worker_respawn_delay().unwrap_or(LogicFrames::ZERO),
                    module.regen_per_second(),
                )
            });
        let rebuild_at = now.checked_add(delay).unwrap_or(Frame::FOREVER);
        tick.state_mut().insert(
            ids.rebuild_hole,
            hole,
            RebuildHole {
                structure: exposure.structure,
                rebuild_at,
                reconstruction: None,
                regen_per_second: regen,
                faded_in_at: now.checked_add(exposure.fade_in).unwrap_or(Frame::FOREVER),
            },
        )?;
        if exposure.transfer_attackers {
            self.transfer_attackers(tick, ids, exposure.died, hole)?;
        }
        diag!(
            "rebuild: entity {} left a hole (entity {}) at ({:.0}, {:.0}) with {:.0} health, \
             frame {}, rebuilding at frame {}",
            exposure.died.index(),
            hole.index(),
            exposure.at.x,
            exposure.at.y,
            exposure.max_health,
            now.get(),
            rebuild_at.get()
        );
        Ok(())
    }

    pub(super) fn turn_to_rubble(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        died: &[Entity],
    ) -> Result<()> {
        let structures: Vec<Entity> = died
            .iter()
            .copied()
            .filter(|entity| {
                tick.state()
                    .get(ids.shape, *entity)
                    .is_some_and(|shape| shape.kind_of.contains(mint_sage_asset::KindOf::STRUCTURE))
            })
            .collect();
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let mut left_wall_layer: Vec<Entity> = Vec::new();
        if let Some(wall_clock) = tick
            .state()
            .store(ids.wall_layer)
            .and_then(|store| store.entities().first().copied())
        {
            for &entity in &structures {
                let holds = tick
                    .state()
                    .get(ids.wall_layer, wall_clock)
                    .is_some_and(|layer| layer.holds(entity));
                if !holds {
                    continue;
                }
                self.drop_wall_occupants(tick, ids, entity);
                if let Some(mut layer) = tick.state().get(ids.wall_layer, wall_clock).cloned() {
                    layer.unstamp(geometry, extent, entity);
                    self.rebuild_reachability(
                        &layer,
                        crate::obstacles::scaleable_of(tick.state(), ids.obstacles),
                    );
                    tick.state_mut().insert(ids.wall_layer, wall_clock, layer)?;
                }
                left_wall_layer.push(entity);
            }
        }
        if let Some(clock) = tick
            .state()
            .store(ids.obstacles)
            .and_then(|store| store.entities().first().copied())
        {
            if let Some(mut layer) = tick
                .state()
                .get(ids.obstacles, clock)
                .filter(|layer| structures.iter().any(|entity| layer.holds(*entity)))
                .cloned()
            {
                for entity in structures {
                    layer.make_rubble(&self.passability.grid, geometry, extent, entity);
                }
                tick.state_mut().insert(ids.obstacles, clock, layer)?;
            }
        }
        for entity in left_wall_layer {
            self.stamp_bookend(tick, ids, entity)?;
        }
        Ok(())
    }

    fn stamp_bookend(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let Some(footprint) = tick
            .state()
            .get(ids.kind, entity)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .and_then(|entry| crate::construction::bookend_footprint(&entry.record))
        else {
            return Ok(());
        };
        let Some(body) = tick.state().get(ids.body, entity).copied() else {
            return Ok(());
        };
        let Some(clock_entity) = tick
            .state()
            .store(ids.obstacles)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(mut layer) = tick.state().get(ids.obstacles, clock_entity).cloned() else {
            return Ok(());
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let scaleable = tick
            .state()
            .get(ids.kind, entity)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .is_some_and(|entry| entry.record.has_kind("SCALEABLE_WALL"));
        if let Err(message) = layer.stamp(
            &self.passability.grid,
            geometry,
            extent,
            entity,
            body.0.position,
            body.0.facing.radians(),
            vec![footprint],
            scaleable,
        ) {
            diag!(
                "obstacles: bookend for entity {}: {message}",
                entity.index()
            );
        }
        tick.state_mut()
            .insert(ids.obstacles, clock_entity, layer)?;
        Ok(())
    }

    fn take_over_geometry(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        exposure: &HoleExposure,
        hole: Entity,
    ) -> Result<()> {
        let taken = tick
            .state()
            .store(ids.obstacles)
            .and_then(|store| store.entities().first().copied())
            .and_then(|clock| tick.state().get(ids.obstacles, clock))
            .and_then(|layer| layer.records_of(exposure.died));
        self.unstamp_footprint(tick, ids, exposure.died)?;
        if let Some(records) = taken {
            self.unstamp_footprint(tick, ids, hole)?;
            self.stamp_footprint(
                tick,
                ids,
                hole,
                exposure.at,
                exposure.facing.radians(),
                records,
            )?;
        }
        tick.state_mut().insert(
            ids.geometry_from,
            hole,
            crate::unit::GeometryFrom(exposure.structure),
        )?;
        let theirs = tick.state().get(ids.shape, exposure.died).copied();
        if let (Some(theirs), Some(mut shape)) =
            (theirs, tick.state().get(ids.shape, hole).copied())
        {
            shape.radius = theirs.radius;
            shape.height = theirs.height;
            tick.state_mut().insert(ids.shape, hole, shape)?;
        }
        Ok(())
    }

    fn transfer_attackers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        from: Entity,
        to: Entity,
    ) -> Result<()> {
        let attackers: Vec<Entity> = tick
            .state()
            .store(ids.attack_order)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, order)| order.target == from)
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for attacker in attackers {
            if let Some(order) = tick.state_mut().get_mut(ids.attack_order, attacker) {
                order.target = to;
            }
            if let Some(armed) = tick.state_mut().get_mut(ids.armed, attacker) {
                if armed.target == Some(from) {
                    armed.target = Some(to);
                }
            }
        }
        Ok(())
    }

    pub(super) fn advance_rebuild_holes(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let holes: Vec<Entity> = tick
            .state()
            .store(ids.rebuild_hole)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let now = tick.frame();
        for hole in holes {
            if !tick.state().is_live(hole) || tick.state().contains(ids.doomed, hole) {
                continue;
            }
            let Some(state) = tick.state().get(ids.rebuild_hole, hole).copied() else {
                continue;
            };
            if state.regen_per_second > 0.0 {
                if let Some(health) = tick.state_mut().get_mut(ids.health, hole) {
                    let step =
                        health.0.max() * state.regen_per_second * LogicFrames::ONE.as_seconds();
                    let current = health.0.current();
                    health.0.set_current(current + step);
                }
            }
            if let Some(site) = state.reconstruction {
                self.watch_reconstruction(tick, ids, hole, state, site)?;
                continue;
            }
            if now.get() < state.rebuild_at.get() {
                continue;
            }
            self.rebuild_from_hole(tick, ids, hole, state)?;
        }
        Ok(())
    }

    fn watch_reconstruction(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        hole: Entity,
        mut state: RebuildHole,
        site: Entity,
    ) -> Result<()> {
        let world = tick.state();
        let standing = world.is_live(site)
            && !world.contains(ids.doomed, site)
            && world
                .get(ids.health, site)
                .is_none_or(|health| !health.0.is_dead());
        let now = tick.frame();
        if standing {
            if world.contains(ids.construction, site) {
                return Ok(());
            }
            tick.state_mut().insert(
                ids.lifetime,
                hole,
                crate::summons::Lifetime {
                    dies_at: now,
                    set_at: None,
                    death: crate::combat::DeathNaming::Faded,
                },
            )?;
            diag!(
                "rebuild: hole {} goes, its structure (entity {}) finished, frame {}",
                hole.index(),
                site.index(),
                now.get()
            );
            return Ok(());
        }
        self.set_mask(tick, ids, hole, false)?;
        let footprint = tick.state().get(ids.body, hole).map(|body| body.0).zip(
            Self::geometry_kind(tick.state(), ids, hole)
                .and_then(|kind| self.kinds.get(kind))
                .map(|entry| crate::construction::footprint_of(&entry.record)),
        );
        if let Some((body, records)) = footprint.filter(|(_, records)| !records.is_empty()) {
            self.stamp_footprint(
                tick,
                ids,
                hole,
                body.position,
                body.facing.radians(),
                records,
            )?;
        }
        let delay = self
            .modules
            .as_ref()
            .and_then(|wiring| wiring.rebuild_hole(tick.state(), hole))
            .and_then(mint_sage_modules::RebuildHoleBehavior::worker_respawn_delay)
            .unwrap_or(LogicFrames::ZERO);
        state.reconstruction = None;
        state.rebuild_at = now.checked_add(delay).unwrap_or(Frame::FOREVER);
        tick.state_mut().insert(ids.rebuild_hole, hole, state)?;
        diag!(
            "rebuild: hole {} unmasked, its structure (entity {}) destroyed while building, \
             frame {}, rebuilding at frame {}",
            hole.index(),
            site.index(),
            now.get(),
            state.rebuild_at.get()
        );
        Ok(())
    }

    fn set_mask(&self, tick: &mut Tick<'_>, ids: Ids, hole: Entity, on: bool) -> Result<()> {
        let statuses = mint_sage_modules::effects::names(
            MASK_STATUSES
                .iter()
                .map(|status| (*status).to_owned())
                .collect(),
        );
        let none = mint_sage_modules::effects::names(Vec::new());
        let (set, clear) = if on {
            (statuses, none)
        } else {
            (none, statuses)
        };
        if on {
            tick.state_mut().remove(ids.selected, hole);
        }
        self.apply_effects(
            tick,
            ids,
            vec![(
                hole,
                mint_sage_modules::effects::Effect::StatusBits { set, clear },
            )],
        )
    }

    fn rebuild_from_hole(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        hole: Entity,
        mut state: RebuildHole,
    ) -> Result<()> {
        let Some(entry) = self.kinds.get(state.structure) else {
            return Ok(());
        };
        let (Some(body), Some(owner)) = (
            tick.state().get(ids.body, hole).map(|body| body.0),
            tick.state().get(ids.owner, hole).map(|owner| owner.0),
        ) else {
            return Ok(());
        };
        let frames = entry
            .record
            .rebuild_seconds
            .or(entry.record.build_time)
            .filter(|seconds| (0.0..=crate::templates::MAX_BUILD_SECONDS).contains(seconds))
            .map_or(1, |seconds| LogicFrames::from_seconds(seconds).get())
            .max(1);
        let worker_kind = self
            .modules
            .as_ref()
            .and_then(|wiring| wiring.rebuild_hole(tick.state(), hole))
            .and_then(mint_sage_modules::RebuildHoleBehavior::worker_object_name)
            .and_then(|name| self.kinds.kind(name));
        self.unstamp_footprint(tick, ids, hole)?;
        let Some(structure) = self.spawn_kind_at(
            tick,
            ids,
            state.structure,
            body.position,
            body.facing,
            owner,
        )?
        else {
            return Ok(());
        };
        if let Some(mut health) = tick.state().get(ids.health, structure).copied() {
            health.0.set_current(1.0);
            tick.state_mut().insert(ids.health, structure, health)?;
        }
        let worker = self.spawn_site_worker(tick, ids, worker_kind, structure, owner)?;
        tick.state_mut().insert(
            ids.construction,
            structure,
            Construction {
                builder: None,
                elapsed: 0,
                rebuild: Some(frames),
                worker,
                delay: 0,
            },
        )?;
        self.transfer_attackers(tick, ids, hole, structure)?;
        self.set_mask(tick, ids, hole, true)?;
        state.reconstruction = Some(structure);
        state.rebuild_at = Frame::FOREVER;
        tick.state_mut().insert(ids.rebuild_hole, hole, state)?;
        diag!(
            "rebuild: hole {} rebuilds {} (entity {}) for player {} over {frames} frame(s), frame {}",
            hole.index(),
            entry.record.name,
            structure.index(),
            owner.get(),
            tick.frame().get()
        );
        Ok(())
    }

    pub(super) fn rebuild_disallowed(
        &self,
        world: &World,
        ids: Ids,
        record: &mint_sage_modules::ObjectRecord,
        dying: Entity,
        at: mint_movement::Vec2,
    ) -> bool {
        let Some(range) = record.disallow_rebuild_range else {
            return false;
        };
        let filter = ObjectFilter::parse(&record.disallow_rebuild_filter);
        let reach = range * range;
        world.store(ids.body).is_some_and(|store| {
            store.iter().any(|(other, body)| {
                other != dying
                    && !world.contains(ids.doomed, other)
                    && body.0.position.minus(at).length_squared() <= reach
                    && world
                        .get(ids.kind, other)
                        .and_then(|kind| self.kinds.get(*kind))
                        .is_some_and(|entry| {
                            filter.passes(&entry.record.kind_of, &entry.record.name)
                        })
            })
        })
    }
}
