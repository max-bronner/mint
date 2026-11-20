use super::*;
use crate::diag::diag;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Harvesting {
    dock: Entity,
    dock_pos: Vec2,
    dock_reach: f32,
    boxes: u32,
    stage: Stage,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    ToTree { tree: Vec2, walking: bool },
    Preparing { tree: Vec2, until: Frame },
    Chopping { tree: Vec2, until: Frame },
    ToDock { walking: bool },
    Banking { until: Frame },
}

impl Component for Harvesting {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.dock.hash_into(hasher);
        hasher.write_u32(self.dock_pos.x.to_bits());
        hasher.write_u32(self.dock_pos.y.to_bits());
        hasher.write_u32(self.dock_reach.to_bits());
        hasher.write_u32(self.boxes);
        match self.stage {
            Stage::ToTree { tree, walking } => {
                hasher.write_u8(0);
                hasher.write_u32(tree.x.to_bits());
                hasher.write_u32(tree.y.to_bits());
                hasher.write_u8(u8::from(walking));
            }
            Stage::Preparing { tree, until } => {
                hasher.write_u8(1);
                hasher.write_u32(tree.x.to_bits());
                hasher.write_u32(tree.y.to_bits());
                until.hash_into(hasher);
            }
            Stage::Chopping { tree, until } => {
                hasher.write_u8(2);
                hasher.write_u32(tree.x.to_bits());
                hasher.write_u32(tree.y.to_bits());
                until.hash_into(hasher);
            }
            Stage::ToDock { walking } => {
                hasher.write_u8(3);
                hasher.write_u8(u8::from(walking));
            }
            Stage::Banking { until } => {
                hasher.write_u8(4);
                until.hash_into(hasher);
            }
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn after_ms(now: Frame, ms: f32) -> Frame {
    let ms = ms.max(0.0).min(f64::from(u32::MAX) as f32) as u32;
    now.checked_add(mint_sage_modules::retail_duration_ticks(ms))
        .unwrap_or(Frame::FOREVER)
}

impl RotwkRules {
    pub(crate) const fn with_value_per_supply_box(mut self, value: f32) -> Self {
        self.value_per_supply_box = value;
        self
    }

    fn can_reach(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        from: Vec2,
        target: Vec2,
        reach: f32,
        mover: MoverClass,
    ) -> bool {
        let (Some(start), Some(goal)) = (
            self.passable_cell_for(tick, ids, from, mover),
            self.passable_cell_near(tick, ids, target, reach, mover),
        ) else {
            return false;
        };
        self.reachable(mover, start, goal)
    }

    pub(super) fn advance_harvesting(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return Ok(());
        };
        let docks: Vec<(Entity, PlayerId, Vec2, f32)> = living
            .iter()
            .filter(|(foe, _)| foe.is_structure)
            .filter_map(|(foe, owner)| {
                self.record_of(tick, ids, foe.entity)
                    .filter(|record| record.has_kind("SUPPLY_GATHERING_CENTER"))
                    .map(|_| (foe.entity, *owner, foe.position, foe.radius))
            })
            .collect();
        for (foe, owner) in living {
            if tick.state().contains(ids.harvesting, foe.entity)
                || tick.state().contains(ids.build_task, foe.entity)
                || tick.state().contains(ids.site_worker, foe.entity)
            {
                continue;
            }
            let harvest = self
                .ai_table_of(tick, ids, foe.entity)
                .and_then(|ai| ai.harvest);
            let Some(harvest) = harvest.filter(|h| h.max_boxes > 0) else {
                continue;
            };
            let Some(&(dock, _, dock_pos, dock_reach)) = docks
                .iter()
                .filter(|(_, dock_owner, _, _)| dock_owner == owner)
                .min_by(|a, b| {
                    a.2.minus(foe.position)
                        .length_squared()
                        .total_cmp(&b.2.minus(foe.position).length_squared())
                })
            else {
                diag!(
                    "forest: harvester {} found no friendly SUPPLY_GATHERING_CENTER dock \
                     ({} total this tick)",
                    foe.entity.index(),
                    docks.len()
                );
                continue;
            };
            self.assign_tree(
                tick,
                ids,
                forest_entity,
                foe.entity,
                Harvesting {
                    dock,
                    dock_pos,
                    dock_reach,
                    boxes: 0,
                    stage: Stage::ToDock { walking: false },
                },
                harvest,
            )?;
        }
        let workers: Vec<Entity> = tick
            .state()
            .store(ids.harvesting)
            .map_or_else(Vec::new, |store| store.entities().to_vec());
        for worker in workers {
            self.advance_harvester(tick, ids, forest_entity, worker)?;
        }
        Ok(())
    }

    fn assign_tree(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        forest_entity: Entity,
        worker: Entity,
        base: Harvesting,
        harvest: mint_sage_modules::HarvestDef,
    ) -> Result<()> {
        let Some(from) = tick
            .state()
            .get(ids.body, worker)
            .map(|body| body.0.position)
        else {
            return Ok(());
        };
        let Some(forest) = tick.state().get(ids.forest, forest_entity) else {
            return Ok(());
        };
        let mover = self.mover_class(tick, ids, worker);
        let candidates = forest.harvestable_within(base.dock_pos, harvest.scan_distance);
        let candidate_count = candidates.len();
        let Some((_, tree)) = candidates.into_iter().find(|(_, pos)| {
            self.can_reach(tick, ids, from, *pos, harvest.activation_range, mover)
        }) else {
            diag!(
                "forest: harvester {} found no reachable harvestable tree \
                 ({candidate_count} within {} of its dock)",
                worker.index(),
                harvest.scan_distance
            );
            return Ok(());
        };
        tick.state_mut().insert(
            ids.harvesting,
            worker,
            Harvesting {
                stage: Stage::ToTree {
                    tree,
                    walking: false,
                },
                ..base
            },
        )?;
        Ok(())
    }

    fn advance_harvester(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        forest_entity: Entity,
        worker: Entity,
    ) -> Result<()> {
        let Some(mut cycle) = tick.state().get(ids.harvesting, worker).copied() else {
            return Ok(());
        };
        let Some(position) = tick
            .state()
            .get(ids.body, worker)
            .map(|body| body.0.position)
        else {
            return Ok(());
        };
        let now = tick.frame();
        let harvest = self
            .ai_table_of(tick, ids, worker)
            .and_then(|ai| ai.harvest)
            .unwrap_or(mint_sage_modules::HarvestDef {
                max_boxes: 1,
                scan_distance: 0.0,
                activation_range: 0.0,
                preparation_time_ms: 0.0,
                action_time_ms: 0.0,
                action_delay_ms: 0.0,
            });
        match cycle.stage {
            Stage::ToTree { tree, mut walking } => {
                let reach = harvest.activation_range;
                cycle.stage =
                    if self.approach(tick, ids, worker, tree, reach, reach, &mut walking)? {
                        Stage::Preparing {
                            tree,
                            until: after_ms(now, harvest.preparation_time_ms),
                        }
                    } else {
                        Stage::ToTree { tree, walking }
                    };
            }
            Stage::Preparing { tree, until } => {
                let Some(forest) = tick.state_mut().get_mut(ids.forest, forest_entity) else {
                    return Ok(());
                };
                let handle = forest.find_closest(
                    tree,
                    harvest.activation_range,
                    false,
                    crate::forest::TreeQuery::Harvestable,
                );
                match handle {
                    None => {
                        tick.state_mut().remove(ids.harvesting, worker);
                        return Ok(());
                    }
                    Some(handle) => {
                        let already_toppled = forest.tree(handle).is_some_and(|tree| tree.toppled);
                        if already_toppled {
                            cycle.stage = Stage::Chopping {
                                tree,
                                until: after_ms(now, harvest.action_time_ms),
                            };
                        } else if now.since(until).is_some() {
                            forest.topple_at(handle, position, crate::forest::NO_SPEED_OVERRIDE);
                            diag!("forest: harvester {} knocked a tree down", worker.index());
                            cycle.stage = Stage::Chopping {
                                tree,
                                until: after_ms(now, harvest.action_time_ms),
                            };
                        }
                    }
                }
            }
            Stage::Chopping { tree, until } => {
                if now.since(until).is_some() {
                    let Some(forest) = tick.state_mut().get_mut(ids.forest, forest_entity) else {
                        return Ok(());
                    };
                    let handle = forest.find_closest(
                        tree,
                        harvest.activation_range,
                        false,
                        crate::forest::TreeQuery::Harvestable,
                    );
                    let wanted = harvest.max_boxes.saturating_sub(cycle.boxes);
                    let taken = handle.map_or(0, |handle| forest.take_supply(handle, wanted));
                    cycle.boxes = cycle.boxes.saturating_add(taken);
                    let tree_gone = handle.is_none() || taken < wanted;
                    if cycle.boxes >= harvest.max_boxes || tree_gone {
                        cycle.stage = Stage::ToDock { walking: false };
                    } else {
                        cycle.stage = Stage::Chopping {
                            tree,
                            until: after_ms(now, harvest.action_time_ms),
                        };
                    }
                }
            }
            Stage::ToDock { mut walking } => {
                let arrive = cycle.dock_reach.max(harvest.activation_range);
                let (dock_pos, dock_reach) = (cycle.dock_pos, cycle.dock_reach);
                cycle.stage = if self.approach(
                    tick,
                    ids,
                    worker,
                    dock_pos,
                    arrive,
                    dock_reach,
                    &mut walking,
                )? {
                    Stage::Banking {
                        until: after_ms(now, harvest.action_delay_ms),
                    }
                } else {
                    Stage::ToDock { walking }
                };
            }
            Stage::Banking { until } => {
                if now.since(until).is_some() {
                    if cycle.boxes > 0 {
                        self.bank_harvest(tick, ids, worker, cycle.dock, cycle.boxes)?;
                        cycle.boxes = 0;
                    }
                    let mover = self.mover_class(tick, ids, worker);
                    let Some(forest) = tick.state().get(ids.forest, forest_entity) else {
                        tick.state_mut().remove(ids.harvesting, worker);
                        return Ok(());
                    };
                    let candidates =
                        forest.harvestable_within(cycle.dock_pos, harvest.scan_distance);
                    let next = candidates.into_iter().find(|(_, pos)| {
                        self.can_reach(
                            tick,
                            ids,
                            cycle.dock_pos,
                            *pos,
                            harvest.activation_range,
                            mover,
                        )
                    });
                    match next {
                        Some((_, tree)) => {
                            cycle.stage = Stage::ToTree {
                                tree,
                                walking: false,
                            };
                        }
                        None => {
                            tick.state_mut().remove(ids.harvesting, worker);
                            return Ok(());
                        }
                    }
                }
            }
        }
        tick.state_mut().insert(ids.harvesting, worker, cycle)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn approach(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        worker: Entity,
        goal: Vec2,
        arrive: f32,
        order_reach: f32,
        walking: &mut bool,
    ) -> Result<bool> {
        let Some(position) = tick
            .state()
            .get(ids.body, worker)
            .map(|body| body.0.position)
        else {
            return Ok(false);
        };
        if position.minus(goal).length() <= arrive {
            return Ok(true);
        }
        if tick.state().contains(ids.path, worker) {
            *walking = true;
            return Ok(false);
        }
        if *walking {
            return Ok(true);
        }
        self.order_builder_to_site(tick, ids, worker, goal, order_reach)?;
        *walking = tick.state().contains(ids.path, worker);
        Ok(false)
    }

    fn bank_harvest(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        worker: Entity,
        dock: Entity,
        boxes: u32,
    ) -> Result<()> {
        let Some(owner) = tick.state().get(ids.owner, worker).map(|owner| owner.0) else {
            return Ok(());
        };
        let economy = Self::economy_ids(ids);
        let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, owner)
        else {
            return Ok(());
        };
        let production = tick
            .state()
            .get(ids.bonuses, dock)
            .map_or(1.0, |bonuses| bonuses.0.production);
        let dock_def = self
            .record_of(tick, ids, dock)
            .and_then(|record| record.supply_center_dock.as_ref());
        let value_multiplier = dock_def.map_or(1.0, |def| def.value_multiplier);
        let money_mult = self.multiplay.money(self.players);
        #[allow(clippy::cast_precision_loss)]
        let per_box = boxes as f32 * self.value_per_supply_box * value_multiplier;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let paid = per_box.max(0.0) as i64;
        let deposit = crate::economy::deposited(paid, money_mult, production);
        let bonus_science = dock_def.and_then(|def| {
            let science = self.sciences.id(def.bonus_science.as_deref()?)?;
            self.science_of(tick.state(), ids, owner)
                .filter(|held| held.holds(science))
                .map(|_| def.bonus_science_multiplier)
        });
        let deposit = bonus_science.map_or(deposit, |multiplier| {
            crate::experience::scaled_amount(deposit, multiplier).unwrap_or(deposit)
        });
        let mut purse = tick
            .state()
            .get(ids.purse, player_entity)
            .map_or(mint_sage_economy::Money::new(0), |held| held.0);
        purse.deposit(deposit.max(0));
        tick.state_mut()
            .insert(ids.purse, player_entity, Purse(purse))?;
        crate::match_stats::record(tick, ids.match_stats, player_entity, |stats| {
            stats.earned = stats.earned.saturating_add(deposit.max(0));
        });
        diag!(
            "forest: harvester {} banked {boxes} box(es) for {deposit}",
            worker.index()
        );
        Ok(())
    }
}
