use super::*;
use mint_sage_combat::{CommandSource, ShotOutcome};
use mint_sage_modules::object_filter::ObjectFilter;
use mint_sage_modules::Effect;

use super::charge_pass::ChargeStep;
use super::fire_gate::FireGate;
use super::loose_melee::LooseStep;

const CLOSE_RANGE: &str = "CLOSE_RANGE";

const APPROACH_COST_WEIGHT: f32 = 0.0005;

const ATTACK_NEEDS_LINE_OF_SIGHT: &str = "ATTACK_NEEDS_LINE_OF_SIGHT";

const CAN_SHOOT_OVER_WALLS: &str = "CAN_SHOOT_OVER_WALLS";

#[cfg(test)]
thread_local! {
    static FORCE_VICTIM_ON_WALL_LAYER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn force_victim_on_wall_layer(force: bool) {
    FORCE_VICTIM_ON_WALL_LAYER.with(|cell| cell.set(force));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplanCadence {
    Normal,
    Boarding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteEnd {
    Reach,
    Closest,
    Spot,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ArmedFacts {
    pub(super) me: Foe,
    pub(super) my_player: PlayerId,
    pub(super) engaged_member: bool,
    pub(super) slots: Option<(usize, super::upgrades::SlotSet)>,
    pub(super) span: Option<(f32, f32, mint_sage_combat::AntiMask)>,
    pub(super) stands: bool,
    pub(super) stopped: bool,
}

pub(super) struct ShotRoll {
    pub(super) outcome: ShotOutcome,
    pub(super) dodged: bool,
    pub(super) infantry: bool,
    pub(super) passenger_roll: Option<bool>,
}

impl RotwkRules {
    pub(super) fn decide_armed(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        living: &[(Foe, PlayerId)],
        formations_on: bool,
    ) -> Option<ArmedFacts> {
        let world = tick.state();
        if world.contains(ids.no_attack, entity)
            || world.contains(ids.using_ability, entity)
            || world.contains(ids.no_auto_acquire, entity)
            || Self::emotion_holds(world, ids, entity)
            || Self::ai_busy(world, ids, entity, tick.frame())
            || Self::deploy_transition(world, ids, entity)
            || self
                .contained_status(world, ids, entity)
                .is_some_and(|status| {
                    !status.has(super::contain::ContainStatus::CAN_ATTACK)
                        || !self
                            .ai_table_of(tick, ids, entity)
                            .is_some_and(|ai| ai.can_attack_while_contained)
                })
        {
            return None;
        }
        let state = world.get(ids.armed, entity)?;
        let engaged_member = formations_on && world.contains(ids.member, entity);
        let Some((me, my_player)) = living
            .binary_search_by_key(&entity, |(foe, _)| foe.entity)
            .ok()
            .and_then(|at| living.get(at))
            .copied()
            .or_else(|| {
                let owner = world.get(ids.owner, entity)?.0;
                Some((self.foe_of(tick, ids, entity)?, owner))
            })
        else {
            return None;
        };
        let slots = self.armed_slots(tick, ids, entity);
        let span = self.combat.weapons.get(state.weapon).map(|template| {
            let own = (
                self.min_range_of(world, entity, template),
                self.attack_range_of(world, ids, entity, template),
                template.anti_mask,
            );
            match slots {
                None => own,
                Some((_, slots)) => {
                    slots
                        .arms
                        .iter()
                        .flatten()
                        .fold(own, |(min, max, mask), arm| {
                            match self.combat.weapons.get(arm.weapon) {
                                Some(template) if !template.bombard_type => (
                                    min.min(self.min_range_of(world, entity, template)),
                                    max.max(self.attack_range_of(world, ids, entity, template)),
                                    mask.union(template.anti_mask),
                                ),
                                _ => (min, max, mask),
                            }
                        })
                }
            }
        });
        Some(ArmedFacts {
            me,
            my_player,
            engaged_member,
            slots,
            span,
            stands: self.holds_ground(world, ids, entity),
            stopped: world.contains(ids.player_stopped, entity)
                && !self.stands_ground(world, ids, entity),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_weapons(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut Vec<DamageEvent>,
        budgets: &mut PathBudgets,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
        global_max_radius: f32,
        now: Frame,
    ) -> Result<()> {
        self.end_one_shot_orders(tick, ids)?;

        self.advance_order_queues(tick, ids, budgets)?;
        self.advance_group_attacks(tick, ids, budgets, grid, living, global_max_radius, now)?;
        self.advance_loose_attack_moves(tick, ids, budgets)?;

        let armed: Vec<Entity> = tick
            .state()
            .store(ids.armed)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|entity| !self.ai_disabled(tick.state(), ids, *entity, now))
                    .collect()
            })
            .unwrap_or_default();
        if armed.is_empty() {
            return Ok(());
        }
        crate::perf::note_combat_counts(armed.len(), living.len());
        let formations_on = self.modes(tick, ids).formation == FormationMode::On;
        self.advance_dual_weapons(tick, ids, living, grid, global_max_radius, now)?;
        self.reload_idle_weapons(tick, ids, &armed, now)?;
        self.advance_turrets(tick, ids, &armed, formations_on, now)?;

        let mut candidate_indices: Vec<u32> = Vec::new();
        let mut candidates: Vec<Foe> = Vec::new();
        let mut aim_effects: Vec<(Entity, Effect)> = Vec::new();

        let _perf = crate::perf::sub(crate::perf::sub_row!("armed_loop"));
        let decide_perf = crate::perf::sub(crate::perf::sub_row!("decide_armed"));
        let decided: Vec<Option<ArmedFacts>> = armed
            .iter()
            .map(|&entity| self.decide_armed(tick, ids, entity, living, formations_on))
            .collect();
        drop(decide_perf);
        for (entity, decision) in armed.into_iter().zip(decided) {
            let choosing = crate::perf::sub(crate::perf::sub_row!("armed_choose"));
            debug_assert_eq!(
                format!("{decision:?}"),
                format!(
                    "{:?}",
                    self.decide_armed(tick, ids, entity, living, formations_on)
                ),
                "an earlier armed turn changed entity {}'s decision",
                entity.index()
            );
            let Some(decided) = decision else {
                continue;
            };
            let Some(mut state) = tick.state().get(ids.armed, entity).copied() else {
                continue;
            };
            let ArmedFacts {
                me,
                my_player,
                engaged_member,
                slots,
                span,
                stands,
                stopped,
            } = decided;
            let previous_next_acquire = state.next_acquire;
            let part = crate::perf::sub(crate::perf::sub_row!("choose_bird_ground"));
            let engagement = self.giant_bird_engagement(tick, ids, entity);
            let bird_strike = match engagement {
                Some(super::giant_bird::Engagement::Busy { target }) => {
                    if state.target != target {
                        state.retarget(target);
                        tick.state_mut().insert(ids.armed, entity, state)?;
                    }
                    let finished = tick
                        .state()
                        .get(ids.attack_order, entity)
                        .is_some_and(|order| !self.swoop_victim_live(tick, ids, order.target));
                    if finished {
                        tick.state_mut().remove(ids.attack_order, entity);
                    }
                    continue;
                }
                Some(super::giant_bird::Engagement::Strike(target)) => Some(target),
                Some(super::giant_bird::Engagement::Free) | None => None,
            };
            if let Some(order) = self.ground_order_of(tick, ids, entity, formations_on) {
                self.fire_at_ground(
                    tick, ids, entity, state, order, &me, living, budgets, events,
                )?;
                continue;
            }
            drop(part);
            let Some((min_range, max_range, anti_mask)) = span else {
                continue;
            };
            let is_melee = |weapon| {
                self.combat
                    .weapons
                    .get(weapon)
                    .is_some_and(|template| template.melee)
            };
            let reach_gated = stands || !self.computer_controlled(my_player);
            let leeching = state.state.leeching(now);
            let holding = self
                .combat
                .weapons
                .get(state.weapon)
                .is_some_and(|template| {
                    after_firing(template, &state, now) == AfterFiring::Holding
                });

            let finishing = self.finishing_victim(tick, ids, &state, living);
            let flagged = self
                .combat
                .weapons
                .get(state.weapon)
                .is_some_and(|template| template.finish_attack_once_started);
            if state.winding && !flagged {
                let abandoned = state.target.is_some_and(|victim| {
                    self.carries_object_status(tick.state(), victim, "FLEE_OFF_MAP")
                });
                if abandoned {
                    state.retarget(None);
                    tick.state_mut().remove(ids.attack_order, entity);
                    tick.state_mut().insert(ids.armed, entity, state)?;
                    continue;
                }
            }

            if let Some(at) = state.position_shot.filter(|_| state.winding) {
                self.continue_position_shot(tick, ids, entity, state, at, &me, living, events)?;
                continue;
            }

            let part = crate::perf::sub(crate::perf::sub_row!("choose_order"));
            let mut ordered = tick.state().get(ids.attack_order, entity).copied();
            if let Some(order) = ordered.filter(|order| order.relation_free) {
                let now_owner = tick
                    .state()
                    .get(ids.owner, order.target)
                    .map(|owner| owner.0);
                let changed = now_owner != order.order_owner;
                let still_non_enemy = !self.object_hostile(tick.state(), entity, order.target);
                if changed && still_non_enemy {
                    tick.state_mut().remove(ids.attack_order, entity);
                    ordered = None;
                }
            }
            let relation_free = ordered.is_some_and(|order| order.relation_free);
            let ordered_foe = |world: &mint_sim::World, target: Entity| {
                living
                    .binary_search_by_key(&target, |(foe, _)| foe.entity)
                    .ok()
                    .and_then(|at| living.get(at))
                    .filter(|(foe, _)| {
                        (relation_free || self.object_hostile(world, entity, foe.entity))
                            && !foe.unattackable
                    })
                    .map(|(foe, _)| *foe)
            };
            let mut ordered_target = ordered
                .map(|order| order.target)
                .and_then(|target| ordered_foe(tick.state(), target));
            if let Some(order) = ordered.filter(|_| ordered_target.is_none()) {
                let repicked = order
                    .horde
                    .and_then(|horde| {
                        self.adjust_victim(
                            tick,
                            ids,
                            entity,
                            horde,
                            None,
                            Some((living, my_player)),
                        )
                    })
                    .and_then(|target| ordered_foe(tick.state(), target));
                if let Some(foe) = repicked {
                    let order = AttackOrder {
                        target: foe.entity,
                        planned: false,
                        ..order
                    };
                    tick.state_mut().insert(ids.attack_order, entity, order)?;
                    ordered = Some(order);
                    ordered_target = Some(foe);
                }
            }
            let continued = ordered
                .filter(|_| ordered_target.is_none())
                .map(|order| order.target)
                .or(state.target.filter(|_| ordered.is_none()))
                .filter(|_| finishing.is_none())
                .and_then(|victim| {
                    self.continued_victim(
                        tick, ids, &state, entity, victim, my_player, anti_mask, living,
                    )
                });
            if let Some(next) = continued {
                match ordered.as_mut() {
                    Some(order) => {
                        order.target = next;
                        tick.state_mut().insert(ids.attack_order, entity, *order)?;
                        ordered_target = ordered_foe(tick.state(), next);
                    }
                    None => state.retarget(Some(next)),
                }
            }
            if ordered.is_some() && ordered_target.is_none() {
                tick.state_mut().remove(ids.attack_order, entity);
            }
            drop(part);

            let part = crate::perf::sub(crate::perf::sub_row!("choose_idle_scan"));
            let moving_under_order = tick
                .state()
                .get(ids.path, entity)
                .is_some_and(|path| path.kind == PathKind::Ordered)
                || tick
                    .state()
                    .get(ids.member, entity)
                    .is_some_and(|member| member.parked);
            let guarded = tick.state().contains(ids.guard, entity);
            let scan = if ordered_target.is_none()
                && !moving_under_order
                && !engaged_member
                && !guarded
                && !self.busy(tick.state(), ids, entity, now)
            {
                let during_idle = !tick.state().contains(ids.attack_move_order, entity);
                self.idle_scan_of(tick, ids, entity, my_player, max_range, during_idle)
            } else {
                None
            };
            let pursues = scan.is_some_and(|scan| scan.pursues);
            drop(part);
            let part = crate::perf::sub(crate::perf::sub_row!("choose_reacquire"));
            let previous_reacquire_ready = state.reacquire_ready;
            let quick_reacquire = if let Some(ready) = state.reacquire_ready {
                now >= ready
            } else {
                let kill_detected = !engaged_member
                    && self
                        .combat
                        .weapons
                        .get(state.weapon)
                        .is_some_and(|template| template.melee)
                    && state
                        .target
                        .or_else(|| {
                            tick.state()
                                .get(ids.loose_melee, entity)
                                .map(|plan| plan.victim())
                        })
                        .is_some_and(|victim| {
                            living
                                .binary_search_by_key(&victim, |(foe, _)| foe.entity)
                                .is_err()
                        });
                if kill_detected {
                    let squish = tick
                        .state()
                        .get(ids.loose_melee, entity)
                        .is_some_and(|plan| plan.squishing())
                        && self
                            .crush_state(tick, ids, entity, me.radius)
                            .is_ready_to_crush();
                    state.squish_reacquire = squish;
                    let retail_frames = if squish {
                        0
                    } else {
                        (crate::game_data::RETAIL_LOGIC_HZ + (entity.index() & 3)) >> 2
                    };
                    let delay_ticks = u64::from(retail_frames)
                        * u64::from(SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ);
                    #[allow(clippy::cast_possible_truncation)]
                    let delay = LogicFrames::from_frames(delay_ticks as u32);
                    state.reacquire_ready = Some(now.checked_add(delay).unwrap_or(now));
                }
                false
            };
            if state.reacquire_ready != previous_reacquire_ready {
                tick.state_mut().insert(ids.armed, entity, state)?;
            }
            drop(part);
            let chosen = if let Some(foe) = finishing {
                Some(foe)
            } else if let Some(victim) = bird_strike {
                living
                    .binary_search_by_key(&victim, |(foe, _)| foe.entity)
                    .ok()
                    .and_then(|at| living.get(at))
                    .map(|(foe, _)| *foe)
            } else if let Some(target) = ordered_target {
                Some(target)
            } else if moving_under_order || guarded {
                None
            } else if engaged_member {
                let melee_horde = tick
                    .state()
                    .get(ids.member, entity)
                    .and_then(|member| tick.state().get(ids.horde, member.horde))
                    .is_some_and(|horde| self.melee_rangefinder(horde).is_some());
                if melee_horde {
                    self.member_attack_victim(tick, ids, &me, living, max_range, now)?
                } else {
                    self.ranged_member_victim(tick, ids, &me, max_range, state.target, living)?
                }
            } else {
                let part = crate::perf::sub(crate::perf::sub_row!("choose_sticky_common"));
                let keep = scan
                    .filter(|scan| scan.pursues)
                    .map_or(max_range, |scan| scan.reach.max(max_range));
                let nearest = if pursues { 0.0 } else { min_range };
                let sticky = state.target.and_then(|current| {
                    let (foe, _) = living
                        .binary_search_by_key(&current, |(foe, _)| foe.entity)
                        .ok()
                        .and_then(|at| living.get(at))
                        .copied()?;
                    if !mint_sage_combat::can_target(
                        mint_sage_combat::Attacker {
                            relation: self.object_victim_relation(tick.state(), entity, foe.entity),
                            class_allowed: target_allowed(anti_mask, &foe),
                            scans_structures: true,
                        },
                        self.seen_victim(tick.state(), ids, my_player, &foe),
                        mint_sage_combat::Mode::AutoAcquire,
                    ) {
                        return None;
                    }
                    (leeching
                        || holding
                        || crate::combat::in_range_volumes(
                            me.reach_volume(),
                            foe.reach_volume(),
                            0.0,
                            nearest,
                            keep,
                        ))
                    .then_some(foe)
                });
                let common = (sticky.is_none() && scan.is_some() && !quick_reacquire)
                    .then(|| self.common_target(tick.state(), ids, entity, living, true))
                    .flatten();
                drop(part);
                let stop_pick = if sticky.is_none()
                    && scan.is_some()
                    && common.is_none()
                    && (quick_reacquire || now >= state.next_acquire)
                {
                    self.stopped_last_damager(tick, ids, entity, &me, living)?
                } else {
                    None
                };
                match (sticky, scan) {
                    (Some(foe), _) => Some(foe),
                    (None, None) => None,
                    (None, Some(_))
                        if !quick_reacquire && self.scanner_off_map(tick.state(), ids, entity) =>
                    {
                        None
                    }
                    (None, Some(_)) if common.is_some() => common,
                    (None, Some(scan)) if stop_pick.is_some() => {
                        if !quick_reacquire {
                            state.next_acquire = now.checked_add(scan.cadence).unwrap_or(now);
                        }
                        stop_pick
                    }
                    (None, Some(_)) if now < state.next_acquire && !quick_reacquire => None,
                    (None, Some(scan)) if scan.retaliate_only && !quick_reacquire => {
                        state.next_acquire = now.checked_add(scan.cadence).unwrap_or(now);
                        self.last_damager(tick.state(), ids, entity, living)
                    }
                    (None, Some(scan)) => 'scan: {
                        let _perf = crate::perf::sub(crate::perf::sub_row!("acquire_scan"));
                        let reach = if quick_reacquire {
                            self.melee_placement.approach_dist
                                * if state.squish_reacquire { 1.0 } else { 2.0 }
                        } else {
                            state.next_acquire = now.checked_add(scan.cadence).unwrap_or(now);
                            scan.reach
                        };
                        if !quick_reacquire {
                            let returned = self.take_last_shot_pick(
                                tick,
                                ids,
                                &mut state,
                                living,
                                &super::last_shot_horde::LastShotScan {
                                    entity,
                                    me: &me,
                                    player: my_player,
                                    scan: &scan,
                                    anti_mask,
                                    reach_gated,
                                    stands,
                                },
                            );
                            if returned.is_some() {
                                break 'scan returned;
                            }
                        }
                        let search_radius =
                            TargetGrid::search_radius(reach, me.radius, global_max_radius);
                        let me_off_map = self.off_map(tick.state(), ids, me.position);
                        let here_reach = (reach_gated && !quick_reacquire)
                            .then(|| self.reach_from_here(tick, ids, entity));
                        let relation = self.relation_from(tick.state(), entity);
                        let high = self.active_high(tick.state(), ids);
                        candidate_indices.clear();
                        grid.query_into(
                            grid_cell(me.position.x),
                            grid_cell(me.position.y),
                            grid_radius_cells(search_radius),
                            &mut candidate_indices,
                        );
                        candidates.clear();
                        for &at in &candidate_indices {
                            #[allow(clippy::cast_possible_truncation)]
                            let at = at as usize;
                            if let Some((foe, _)) = living.get(at).filter(|(foe, _)| {
                                if quick_reacquire {
                                    !foe.is_structure
                                        && super::targeting::outside(high, foe.position)
                                            == me_off_map
                                        && crate::combat::in_range_volumes(
                                            me.reach_volume(),
                                            foe.reach_volume(),
                                            0.0,
                                            0.0,
                                            reach,
                                        )
                                } else {
                                    scan.finds(me.reach_volume(), foe.reach_volume())
                                        && !super::targeting::outside(high, foe.position)
                                        && here_reach.as_ref().is_none_or(|here| {
                                            here.attackable(me.position, me.radius, foe, stands)
                                        })
                                }
                            }) {
                                let relation = relation.to(foe.entity).victim();
                                if crate::combat::auto_acquirable(relation)
                                    && mint_sage_combat::can_target(
                                        mint_sage_combat::Attacker {
                                            relation,
                                            class_allowed: target_allowed(anti_mask, foe),
                                            scans_structures: scan.structures,
                                        },
                                        self.scanned_victim(tick.state(), ids, my_player, foe),
                                        mint_sage_combat::Mode::AutoAcquire,
                                    )
                                {
                                    candidates.push(*foe);
                                }
                            }
                        }
                        let found = self.acquire_for(
                            tick.state(),
                            ids,
                            entity,
                            me.position,
                            me.radius,
                            &candidates,
                            nearest,
                            reach,
                        );
                        found.and_then(|found| {
                            let found = self.horde_member_pick(tick, ids, entity, found);
                            living
                                .binary_search_by_key(&found, |(foe, _)| foe.entity)
                                .ok()
                                .and_then(|at| living.get(at))
                                .map(|(foe, _)| *foe)
                        })
                    }
                }
            };
            let chosen = chosen.filter(|target| target_allowed(anti_mask, target));
            drop(choosing);
            let _engaging = crate::perf::sub(crate::perf::sub_row!("armed_engage"));
            let Some(target) = chosen else {
                let _part = crate::perf::sub(crate::perf::sub_row!("engage_no_target"));
                if !engaged_member
                    && (tick.state().contains(ids.loose_melee, entity)
                        || crate::goal_cells::layer(tick.state(), ids.goal_cells)
                            .is_some_and(|layer| layer.held(entity).is_some()))
                {
                    if tick.state().contains(ids.loose_melee, entity)
                        && tick
                            .state()
                            .get(ids.path, entity)
                            .is_some_and(|path| path.kind == PathKind::Closing)
                    {
                        tick.state_mut().remove(ids.path, entity);
                    }
                    self.end_loose_attack(tick, ids, entity);
                }
                aim_effects.extend(
                    self.end_charge_pass(tick, ids, entity)
                        .map(|effect| (entity, effect)),
                );
                let lost_target = state.target.is_some();
                if lost_target {
                    state.retarget(None);
                    tick.state_mut().remove(ids.approach_wait, entity);
                    aim_effects.extend(self.aim_cleared(tick, entity).map(|e| (entity, e)));
                    if pursues
                        && tick
                            .state()
                            .get(ids.path, entity)
                            .is_some_and(|path| path.kind == PathKind::Closing)
                    {
                        tick.state_mut().remove(ids.path, entity);
                    }
                }
                if lost_target || state.next_acquire != previous_next_acquire {
                    tick.state_mut().insert(ids.armed, entity, state)?;
                }
                continue;
            };
            let part = crate::perf::sub(crate::perf::sub_row!("engage_uncohere"));
            self.attack_state_uncohere(tick, ids, entity, my_player, target.entity);
            drop(part);
            let part = crate::perf::sub(crate::perf::sub_row!("engage_weapon_choice"));

            let (min_range, max_range) = match slots {
                None => (min_range, max_range),
                Some(set) => {
                    let source = if ordered.is_some_and(|order| !order.guard) {
                        CommandSource::Player
                    } else {
                        CommandSource::Ai
                    };
                    let was_melee = is_melee(state.weapon);
                    let aiming = crate::perf::sub(crate::perf::sub_row!("engage_aim_weapon"));
                    let answered =
                        self.aim_weapon(tick, ids, entity, &mut state, set, target.entity, source)?;
                    drop(aiming);
                    if !answered {
                        state.retarget(None);
                        tick.state_mut().remove(ids.attack_order, entity);
                        tick.state_mut().insert(ids.armed, entity, state)?;
                        aim_effects.extend(self.aim_cleared(tick, entity).map(|e| (entity, e)));
                        continue;
                    }
                    if is_melee(state.weapon) != was_melee {
                        self.rebuild_attack(tick, ids, entity, &mut state);
                    }
                    match self.combat.weapons.get(state.weapon) {
                        Some(template) => (
                            self.min_range_of(tick.state(), entity, template),
                            self.attack_range_of(tick.state(), ids, entity, template),
                        ),
                        None => (min_range, max_range),
                    }
                }
            };
            drop(part);
            let part = crate::perf::sub(crate::perf::sub_row!("engage_after_firing"));
            let target = living
                .binary_search_by_key(&target.entity, |(foe, _)| foe.entity)
                .ok()
                .and_then(|at| living.get(at))
                .map_or(target, |(foe, _)| *foe);
            let max_range = self
                .combat
                .weapons
                .get(state.weapon)
                .map_or(max_range, |template| {
                    self.attack_range_at(tick.state(), ids, entity, template, me.z - target.z)
                });
            if bird_strike.is_none() && state.target == Some(target.entity) {
                let after = self
                    .combat
                    .weapons
                    .get(state.weapon)
                    .map_or(AfterFiring::Free, |template| {
                        after_firing(template, &state, now)
                    });
                match after {
                    AfterFiring::Holding => {
                        tick.state_mut().insert(ids.armed, entity, state)?;
                        continue;
                    }
                    AfterFiring::EndsAttack => {
                        if self.holds_temporary_weapon_lock(tick, entity) {
                            aim_effects.push((entity, Effect::ReleaseTemporaryWeaponLock));
                        }
                        state.retarget(None);
                        tick.state_mut().remove(ids.attack_order, entity);
                        tick.state_mut().insert(ids.armed, entity, state)?;
                        aim_effects.extend(self.aim_cleared(tick, entity).map(|e| (entity, e)));
                        continue;
                    }
                    AfterFiring::Free => {}
                }
            }
            if engagement == Some(super::giant_bird::Engagement::Free)
                && (ordered_target.is_some()
                    || pursues
                    || crate::combat::in_range_volumes(
                        me.reach_volume(),
                        target.reach_volume(),
                        0.0,
                        min_range,
                        max_range,
                    ))
            {
                state.retarget(Some(target.entity));
                tick.state_mut().insert(ids.armed, entity, state)?;
                self.begin_giant_bird_swoop(tick, ids, entity, target.entity)?;
                continue;
            }
            let charge = (bird_strike.is_none() && !engaged_member)
                .then(|| self.combat.weapons.get(state.weapon))
                .flatten()
                .filter(|template| template.can_fire_while_charging)
                .map(|template| template.aim_tolerance());
            if charge.is_none() {
                aim_effects.extend(
                    self.end_charge_pass(tick, ids, entity)
                        .map(|effect| (entity, effect)),
                );
            }
            if let Some(aim_tolerance) = charge {
                let step = self.charge_pass_step(
                    tick,
                    ids,
                    entity,
                    &me,
                    &target,
                    ((min_range, max_range), aim_tolerance),
                    budgets,
                )?;
                if step == ChargeStep::Done {
                    aim_effects.extend(
                        self.end_charge_pass(tick, ids, entity)
                            .map(|effect| (entity, effect)),
                    );
                    state.retarget(None);
                    tick.state_mut().remove(ids.attack_order, entity);
                    tick.state_mut().insert(ids.armed, entity, state)?;
                    continue;
                }
                if step == ChargeStep::Run {
                    if state.target != Some(target.entity) {
                        state.retarget(Some(target.entity));
                        tick.state_mut().insert(ids.armed, entity, state)?;
                    }
                    continue;
                }
            }
            drop(part);
            let part = crate::perf::sub(crate::perf::sub_row!("engage_reach"));
            let melee = self
                .combat
                .weapons
                .get(state.weapon)
                .is_some_and(|template| template.melee);
            let pursuing = !melee
                && charge.is_none()
                && !engaged_member
                && self.should_pursue(tick, ids, entity, &me, &target);
            let walking_attack_path = !melee
                && charge.is_none()
                && !engaged_member
                && !pursuing
                && self.walking_attack_path(tick, ids, entity, target.entity);
            let reach = if melee && charge.is_none() {
                self.claim_contact(tick, ids, entity, &me, target, (min_range, max_range))?
            } else {
                target
            };
            let leeching = state.state.leeching(now);
            let locked = (leeching && state.target == Some(target.entity)) || bird_strike.is_some();
            let placed = melee
                && charge.is_none()
                && !engaged_member
                && !locked
                && !target.is_structure
                && reach.position == target.position
                && (ordered_target.is_some() || pursues);
            drop(part);
            let part = crate::perf::sub(crate::perf::sub_row!("engage_close"));
            if stopped
                && melee
                && charge.is_none()
                && !engaged_member
                && !crate::combat::in_range_volumes(
                    me.reach_volume(),
                    target.reach_volume(),
                    0.0,
                    min_range,
                    max_range,
                )
            {
                if tick
                    .state()
                    .get(ids.path, entity)
                    .is_some_and(|path| path.kind == PathKind::Closing)
                {
                    tick.state_mut().remove(ids.path, entity);
                }
                self.end_loose_attack(tick, ids, entity);
                state.retarget(None);
                tick.state_mut().remove(ids.attack_order, entity);
                tick.state_mut().insert(ids.armed, entity, state)?;
                continue;
            }
            let mut running_down = false;
            if placed {
                let step =
                    self.loose_melee_step(tick, ids, entity, &me, &target, max_range, budgets)?;
                running_down = step == LooseStep::Strike;
                if step == LooseStep::Fail {
                    if tick
                        .state()
                        .get(ids.path, entity)
                        .is_some_and(|path| path.kind == PathKind::Closing)
                    {
                        tick.state_mut().remove(ids.path, entity);
                    }
                    self.end_loose_attack(tick, ids, entity);
                    state.retarget(None);
                    tick.state_mut().remove(ids.attack_order, entity);
                    tick.state_mut().insert(ids.armed, entity, state)?;
                    continue;
                }
                if step == LooseStep::Walk {
                    if stands {
                        if tick
                            .state()
                            .get(ids.path, entity)
                            .is_some_and(|path| path.kind == PathKind::Closing)
                        {
                            tick.state_mut().remove(ids.path, entity);
                        }
                        if !stopped
                            && self.attackable_from_here(
                                tick,
                                ids,
                                entity,
                                me.position,
                                me.radius,
                                &target,
                                true,
                            )
                        {
                            if state.target != Some(target.entity) {
                                state.retarget(Some(target.entity));
                                tick.state_mut().insert(ids.armed, entity, state)?;
                            } else if state.next_acquire != previous_next_acquire {
                                tick.state_mut().insert(ids.armed, entity, state)?;
                            }
                            continue;
                        }
                        self.end_loose_attack(tick, ids, entity);
                        state.retarget(None);
                        tick.state_mut().remove(ids.attack_order, entity);
                        tick.state_mut().insert(ids.armed, entity, state)?;
                        continue;
                    }
                    if pursues
                        && ordered_target.is_none()
                        && self.give_up_chase(tick, ids, entity, target.entity, now)
                    {
                        self.end_loose_attack(tick, ids, entity);
                        state.retarget(None);
                        tick.state_mut().insert(ids.armed, entity, state)?;
                        continue;
                    }
                    if pursues {
                        if state.target != Some(target.entity) {
                            state.retarget(Some(target.entity));
                            tick.state_mut().insert(ids.armed, entity, state)?;
                        } else if state.next_acquire != previous_next_acquire {
                            tick.state_mut().insert(ids.armed, entity, state)?;
                        }
                    } else if state.target.is_some() {
                        state.retarget(None);
                        tick.state_mut().insert(ids.armed, entity, state)?;
                    }
                    continue;
                }
            } else if !locked
                && charge.is_none()
                && !(if melee && reach.position == target.position {
                    let _gate = crate::perf::sub(crate::perf::sub_row!("engage_range_gate"));
                    self.melee_in_reach(tick, ids, &me, &target)
                } else {
                    let _gate = crate::perf::sub(crate::perf::sub_row!("engage_range_gate"));
                    crate::combat::in_range_volumes(
                        me.reach_volume(),
                        reach.reach_volume(),
                        0.0,
                        min_range,
                        max_range,
                    ) && (melee || !self.fire_view_blocked(tick, ids, entity, &me, &reach))
                        && !walking_attack_path
                })
            {
                if stands || self.chase_forbidden(tick, ids, entity, my_player, ordered, &state) {
                    state.retarget(None);
                    tick.state_mut().remove(ids.attack_order, entity);
                    tick.state_mut().insert(ids.armed, entity, state)?;
                    aim_effects.extend(self.aim_cleared(tick, entity).map(|e| (entity, e)));
                    continue;
                }
                if pursues
                    && ordered_target.is_none()
                    && self.give_up_chase(tick, ids, entity, target.entity, now)
                {
                    state.retarget(None);
                    tick.state_mut().insert(ids.armed, entity, state)?;
                    continue;
                }
                let due = melee || self.ranged_replan_due(tick, ids, entity, &me, &reach)?;
                if (ordered_target.is_some() || pursues) && due {
                    let _approach = crate::perf::sub(crate::perf::sub_row!("engage_approach"));
                    if melee {
                        let (goal, goal_reach) = match stand_off(
                            me.position,
                            reach.position,
                            me.radius + reach.radius,
                            (min_range, max_range),
                        ) {
                            Some(spot) => (spot, 0.0),
                            None => (reach.position, reach.radius),
                        };
                        self.close_on_target(
                            tick,
                            ids,
                            entity,
                            me.position,
                            goal,
                            goal_reach,
                            PathKind::Closing,
                            budgets,
                        )?;
                    } else if pursuing {
                        self.close_on_target(
                            tick,
                            ids,
                            entity,
                            me.position,
                            reach.position,
                            reach.radius,
                            PathKind::Closing,
                            budgets,
                        )?;
                    } else if self.ranged_approach_step(
                        tick,
                        ids,
                        entity,
                        &me,
                        &reach,
                        (min_range, max_range),
                        budgets,
                    )? {
                        state.retarget(None);
                        tick.state_mut().remove(ids.attack_order, entity);
                        tick.state_mut().insert(ids.armed, entity, state)?;
                        aim_effects.extend(self.aim_cleared(tick, entity).map(|e| (entity, e)));
                        continue;
                    }
                }
                if pursues {
                    if state.target != Some(target.entity) {
                        state.retarget(Some(target.entity));
                        tick.state_mut().insert(ids.armed, entity, state)?;
                    } else if state.next_acquire != previous_next_acquire {
                        tick.state_mut().insert(ids.armed, entity, state)?;
                    }
                    continue;
                }
                if state.target.is_some() {
                    state.retarget(None);
                    tick.state_mut().insert(ids.armed, entity, state)?;
                }
                continue;
            }
            tick.state_mut().remove(ids.approach_wait, entity);
            drop(part);
            let part = crate::perf::sub(crate::perf::sub_row!("engage_settle"));

            if pursuing {
                self.pursue_in_range(tick, ids, entity, &me, &target, budgets)?;
            } else if bird_strike.is_none() && !running_down && charge.is_none() {
                if !melee && !engaged_member {
                    let goal = tick.state().get(ids.path, entity).and_then(|path| {
                        self.ranged_plan_victim_at(tick, ids, entity, path.goal, target.entity)
                    });
                    self.leave_ranged_approach(tick, ids, entity, goal)?;
                }
                self.stop_to_fire(tick, ids, entity)?;
                if !engaged_member {
                    self.claim_spot(tick, ids, entity, me.position);
                }
            }
            if let Some(mut order) = ordered {
                if !order.planned {
                    order.planned = true;
                    tick.state_mut().insert(ids.attack_order, entity, order)?;
                }
            }

            let new_target = state.target != Some(target.entity);
            state.retarget(Some(target.entity));
            if new_target {
                self.diag_contact_engagement(tick, ids, entity, me.position, target.entity);
            }
            drop(part);
            let part = crate::perf::sub(crate::perf::sub_row!("engage_aim"));
            let rate = tick
                .state()
                .get(ids.bonuses, entity)
                .map_or(1.0, |bonuses| bonuses.0.rate_of_fire);
            let gate = if state.winding || bird_strike.is_some() || running_down || charge.is_some()
            {
                FireGate::Open
            } else {
                self.fire_gate(tick, ids, entity, target.entity)
            };
            if gate == FireGate::Refused {
                state.retarget(None);
                tick.state_mut().remove(ids.attack_order, entity);
                tick.state_mut().insert(ids.armed, entity, state)?;
                aim_effects.extend(self.aim_cleared(tick, entity).map(|e| (entity, e)));
                continue;
            }
            let facing_at = self.facing_point(tick, ids, state.weapon, me.position, &target);
            let aimed = if bird_strike.is_some() || running_down || charge.is_some() {
                true
            } else if state.winding {
                self.keep_facing(tick, ids, entity, state.weapon, me.position, facing_at)?;
                true
            } else {
                self.aim_body(tick, ids, entity, state.weapon, me.position, facing_at)?
                    && !self.moving_blocks_fire(tick, ids, entity, state.weapon)
                    && gate == FireGate::Open
            };
            let position_weapon = self.fires_at_position(tick, ids, state.weapon, target.entity);
            let shot = if aimed && !self.wind_up_deferred(tick, ids, entity, &state) {
                tick_weapon(
                    &self.combat,
                    &mut state,
                    now,
                    false,
                    me.position,
                    rate,
                    Self::bonus_conditions(tick.state(), ids, entity),
                    tick.rng(),
                )
            } else {
                crate::combat::ShotEvent::Nothing
            };
            if position_weapon && shot == crate::combat::ShotEvent::WindUpStarted {
                state.position_shot = Some(target.position);
            }
            if shot.fired() && !position_weapon {
                Self::note_shot_horde(tick.state(), ids, &mut state, target.entity);
            }
            let weapon = state.weapon;
            tick.state_mut().insert(ids.armed, entity, state)?;
            drop(part);
            let _part = crate::perf::sub(crate::perf::sub_row!("engage_fire"));
            if shot == crate::combat::ShotEvent::WindUpStarted {
                self.emit_weapon_pre_attack_fx(tick, ids, weapon, me.position, entity);
                aim_effects.extend(
                    self.lock_when_using(tick, ids, entity, weapon)
                        .map(|effect| (entity, effect)),
                );
            }
            if shot == crate::combat::ShotEvent::WindUpStarted || shot.fired() {
                self.note_common_target(tick, ids, entity, target.entity);
            }
            let fired = shot.fired();
            if !fired {
                continue;
            }
            aim_effects.extend(
                self.after_shot_lock(tick, ids, entity, weapon)
                    .map(|effect| (entity, effect)),
            );
            self.note_horde_last_shot(tick, ids, entity);
            if position_weapon {
                self.apply_effects(tick, ids, std::mem::take(&mut aim_effects))?;
                self.fire_at_position(
                    tick,
                    ids,
                    entity,
                    weapon,
                    &me,
                    target.position,
                    living,
                    events,
                )?;
                continue;
            }
            if bird_strike.is_none()
                && charge.is_none()
                && self.fire_linear_volley(tick, ids, entity, weapon, &me, living, events)?
            {
                continue;
            }
            if bird_strike.is_some() {
                self.giant_bird_fired(tick, ids, entity)?;
            }
            if charge.is_some() {
                aim_effects.extend(
                    self.charge_fired(tick, ids, entity, &me, &target, budgets)?
                        .map(|effect| (entity, effect)),
                );
            }
            let ShotRoll {
                outcome,
                dodged,
                infantry,
                passenger_roll,
            } = self.roll_shot(tick, ids, weapon, &target);
            if crate::diag::wants("shot") {
                let kind = tick.state().get(ids.kind, entity).copied();
                let slot = tick
                    .state()
                    .get(ids.weapon_slots, entity)
                    .map(|s| s.current);
                let off = tick.state().get(ids.body, entity).and_then(|body| {
                    Facing::from_vector(target.position.minus(me.position))
                        .map(|towards| body.0.facing.delta_to(towards).abs().to_degrees())
                });
                if let Some(t) = self.combat.weapons.get(weapon) {
                    crate::diag::diag!(
                        "shot: frame {} entity {} {} weapon {} slot {:?} at {} {}{} range {:.0} off {:.1} deg (delay {}-{} pre {} clip {} firing {}) {}",
                        now.get(),
                        entity.index(),
                        self.kind_name(kind),
                        weapon.index(),
                        slot,
                        target.entity.index(),
                        self.kind_name(tick.state().get(ids.kind, target.entity).copied()),
                        if infantry { " (infantry)" } else { "" },
                        (target.position.minus(me.position).length() - me.radius - target.radius)
                            .max(0.0),
                        off.unwrap_or(0.0),
                        t.delay_between_shots.min().get(),
                        t.delay_between_shots.max().get(),
                        t.pre_attack_delay.get(),
                        t.clip_size,
                        t.firing_duration.get(),
                        match outcome {
                            ShotOutcome::Struck => "struck".to_owned(),
                            ShotOutcome::Missed([dx, dy]) => format!(
                                "missed by {:.1}{}",
                                (dx * dx + dy * dy).sqrt(),
                                if dodged { " (dodged)" } else { "" },
                            ),
                        },
                    );
                }
            }
            if let Some((pitch, distance)) = self.aim_of(tick, entity, weapon, &me, &target) {
                if !self.holds_model_condition(tick, entity, pitch.condition()) {
                    crate::diag::diag!("weapon: {} {}", entity.index(), pitch.condition());
                }
                aim_effects.push((entity, aim_condition(Some(pitch), distance)));
            }
            let origin = self.take_launch(tick, entity, &me, None);
            let shot = match outcome {
                _ if charge.is_some() => Shot::at_ground(
                    weapon,
                    entity,
                    origin,
                    me.position,
                    self.ground_height(me.position.x, me.position.y),
                ),
                _ => self.shot_at_foe(weapon, entity, origin, &target, outcome),
            };
            let melee = self
                .combat
                .weapons
                .get(weapon)
                .is_some_and(|template| template.melee);
            let fx = if melee { target.entity } else { entity };
            self.release_shot(
                tick,
                ids,
                &shot,
                fx,
                target.position,
                passenger_roll,
                living,
                events,
            )?;
        }
        self.apply_effects(tick, ids, aim_effects)?;
        Ok(())
    }

    fn ranged_replan_due(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
    ) -> Result<bool> {
        let now = tick.frame();
        let Some(planned) = tick
            .state()
            .get(ids.path, entity)
            .filter(|path| path.kind == PathKind::Closing)
            .map(|path| path.goal)
        else {
            return Ok(true);
        };
        let cooling = tick
            .state()
            .get(ids.replanned_at, entity)
            .is_some_and(|last| {
                now.get().saturating_sub(last.0.get()) < super::movement::REPLAN_COOLDOWN_FRAMES
            });
        if cooling {
            return Ok(false);
        }
        let stood = self
            .ranged_plan_victim_at(tick, ids, entity, planned, victim.entity)
            .unwrap_or(planned);
        if super::movement::goal_moved(me.position, stood, victim.position) {
            return Ok(true);
        }
        tick.state_mut()
            .insert(ids.replanned_at, entity, super::movement::ReplannedAt(now))?;
        Ok(false)
    }

    fn chase_forbidden(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        player: PlayerId,
        order: Option<AttackOrder>,
        state: &Armed,
    ) -> bool {
        if self.computer_controlled(player) || order.is_some_and(|order| !order.guard) {
            return false;
        }
        let Some(template) = self.combat.weapons.get(state.weapon) else {
            return false;
        };
        if template.melee
            || template.chase_weapon
            || self.owns_charge_attack(tick.state(), ids, entity)
        {
            return false;
        }
        !tick
            .state()
            .get(ids.guard, entity)
            .is_some_and(|guard| guard.state == super::guard::GuardState::AttackAggressor)
    }

    fn should_pursue(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
    ) -> bool {
        if tick.state().contains(ids.member, entity)
            || !self.turret_carries_current_weapon(tick, ids, entity)
        {
            return false;
        }
        let Some(body) = tick.state().get(ids.body, victim.entity).map(|body| body.0) else {
            return false;
        };
        let owner = tick.state().get(ids.owner, entity).map(|owner| owner.0);
        if self.guard_ranges.crushes_infantry
            && owner.is_some_and(|owner| self.computer_controlled(owner))
            && self.can_crush_instead(tick, ids, entity, victim.entity)
        {
            return true;
        }
        let too_close = tick
            .state()
            .get(ids.armed, entity)
            .and_then(|armed| self.combat.weapons.get(armed.weapon))
            .map(|template| self.min_range_of(tick.state(), entity, template))
            .is_some_and(|minimum| {
                minimum > 0.0
                    && minimum * minimum
                        > mint_sage_combat::surface_distance_squared_2d(
                            mint_sage_combat::Point3::new(me.position.x, me.position.y, 0.0),
                            me.radius,
                            mint_sage_combat::Point3::new(
                                victim.position.x,
                                victim.position.y,
                                0.0,
                            ),
                            victim.radius,
                        )
            });
        if too_close {
            return false;
        }
        let top = self.mover_limits(tick, ids, entity).max_speed;
        let speed = body.speed;
        if !(top > 0.0 && speed >= top / 10.0 && speed < top) {
            return false;
        }
        victim
            .position
            .minus(me.position)
            .dot(body.facing.to_vector())
            >= 0.0
    }

    fn pursue_in_range(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let speed = tick
            .state()
            .get(ids.body, victim.entity)
            .map_or(0.0, |body| body.0.speed);
        let until = tick
            .frame()
            .get()
            .saturating_add(u64::from(SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ));
        tick.state_mut().insert(
            ids.crush_speed_cap,
            entity,
            crate::combat::CrushSpeedCap {
                cap: speed * PURSUE_IN_RANGE_SPEED,
                until,
            },
        )?;
        if self.ranged_replan_due(tick, ids, entity, me, victim)? {
            self.close_on_target(
                tick,
                ids,
                entity,
                me.position,
                victim.position,
                victim.radius,
                PathKind::Closing,
                budgets,
            )?;
        }
        Ok(())
    }

    pub(super) fn leave_ranged_approach(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        goal: Option<Vec2>,
    ) -> Result<()> {
        let approaching = tick
            .state()
            .get(ids.path, entity)
            .is_some_and(|path| path.kind == PathKind::Closing);
        let Some(goal) = goal.filter(|_| approaching) else {
            return Ok(());
        };
        if tick.state().contains(ids.flight, entity) {
            return Ok(());
        }
        let Some(mut body) = tick.state().get(ids.body, entity).copied() else {
            return Ok(());
        };
        let placed = super::horde_melee::approach_exit_position(body.0.position, goal);
        if placed != body.0.position {
            body.0.position = placed;
            tick.state_mut().insert(ids.body, entity, body)?;
        }
        Ok(())
    }

    pub(super) fn stop_to_fire(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        if !self.cannot_stop(tick, ids, entity) {
            tick.state_mut().remove(ids.path, entity);
            if let Some(mut body) = tick.state().get(ids.body, entity).copied() {
                body.0.speed = 0.0;
                tick.state_mut().insert(ids.body, entity, body)?;
            }
        }
        Ok(())
    }

    pub(super) fn roll_shot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        weapon: WeaponId,
        target: &Foe,
    ) -> ShotRoll {
        let target_entry = tick
            .state()
            .get(ids.kind, target.entity)
            .and_then(|kind| self.kinds.get(*kind));
        let infantry = target_entry
            .is_some_and(|entry| entry.record.kind_of.iter().any(|flag| flag == "INFANTRY"));
        let dodge_percent = target_entry
            .and_then(|entry| entry.record.dodge_percent)
            .unwrap_or(0.0);
        let on_wall = tick
            .state()
            .get(ids.layer, target.entity)
            .is_some_and(|layer| layer.is_wall_top());
        let template = self
            .combat
            .weapons
            .get(weapon)
            .filter(|template| !template.damage_at_self);
        let outcome = template.map_or(ShotOutcome::Struck, |template| {
            template.resolve_shot(tick.rng(), infantry, on_wall)
        });
        let dodged = matches!(outcome, ShotOutcome::Struck)
            && dodge_percent > 0.0
            && template.is_some_and(|template| template.can_be_dodged)
            && tick.rng().next_f32() < dodge_percent;
        let outcome = if dodged {
            mint_sage_combat::scatter_landing(
                tick.rng(),
                template.map_or(0.0, |t| t.scatter_radius),
            )
        } else {
            outcome
        };
        let passenger_roll = template
            .filter(|template| template.hit_passenger_chance > 0.0)
            .filter(|template| template.launches_projectile())
            .map(|template| {
                matches!(outcome, ShotOutcome::Struck)
                    && self
                        .first_passenger(tick.state(), ids, target.entity)
                        .is_some()
                    && tick.rng().next_f32() <= template.hit_passenger_chance
            });
        ShotRoll {
            outcome,
            dodged,
            infantry,
            passenger_roll,
        }
    }

    pub(super) fn shot_at_foe(
        &self,
        weapon: WeaponId,
        attacker: Entity,
        origin: mint_sage_combat::Point3,
        target: &Foe,
        outcome: ShotOutcome,
    ) -> Shot {
        match outcome {
            ShotOutcome::Struck => Shot::at_victim(
                &self.combat,
                weapon,
                attacker,
                origin,
                (target.entity, target.position, target.aim_point().z),
                target.z,
            ),
            ShotOutcome::Missed([dx, dy]) => {
                let landing = Vec2::new(target.position.x + dx, target.position.y + dy);
                Shot::missing(
                    weapon,
                    attacker,
                    origin,
                    target.entity,
                    landing,
                    self.ground_height(landing.x, landing.y),
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn release_shot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        shot: &Shot,
        fx: Entity,
        aimed_at: Vec2,
        passenger_roll: Option<bool>,
        living: &[(Foe, PlayerId)],
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        if self.deploy_holds_fire(tick.state(), ids, shot.attacker) {
            return Ok(());
        }
        self.launch_shot(
            tick,
            ids,
            shot,
            fx,
            aimed_at,
            passenger_roll,
            living,
            events,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn launch_shot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        shot: &Shot,
        fx: Entity,
        aimed_at: Vec2,
        passenger_roll: Option<bool>,
        living: &[(Foe, PlayerId)],
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        let filter = self
            .combat
            .weapons
            .get(shot.weapon)
            .filter(|template| template.launches_projectile())
            .filter(|template| !template.projectile_filter_in_container.is_empty())
            .map(|template| ObjectFilter::parse(&template.projectile_filter_in_container));
        if let Some(filter) = &filter {
            if self
                .passenger_admitted(tick.state(), ids, shot.attacker, filter)
                .is_none()
            {
                crate::diag::diag!(
                    "shot: frame {} entity {} weapon {} holds no passenger its ProjectileFilterInContainer admits; nothing is thrown",
                    tick.frame().get(),
                    shot.attacker.index(),
                    shot.weapon.index()
                );
                return Ok(());
            }
        }
        let mut result = self.fire_shot(tick, ids, shot, living)?;
        Self::defer_self_hits(shot.attacker, &mut result);
        if let Some(at) = Self::fx_point_of(tick.state(), ids, fx) {
            self.emit_weapon_fx(
                tick,
                ids,
                shot.weapon,
                at,
                fx,
                shot.attacker,
                Some(aimed_at),
                shot.victim.map(|(entity, _, _)| entity),
            )?;
        }
        let projectiles = core::mem::take(&mut result.projectiles);
        self.apply_fire_result(tick, ids, shot.weapon, shot, result, events)?;
        let thrown_self = self
            .combat
            .weapons
            .get(shot.weapon)
            .is_some_and(|template| template.projectile_self);
        for pending in projectiles {
            let carried = match &filter {
                None if thrown_self => tick
                    .state()
                    .get(ids.kind, shot.attacker)
                    .map(|kind| (shot.attacker, *kind)),
                None => None,
                Some(filter) => {
                    let world = tick.state();
                    let Some(thrown) = self
                        .passenger_admitted(world, ids, shot.attacker, filter)
                        .and_then(|held| Some((held, *world.get(ids.kind, held)?)))
                    else {
                        continue;
                    };
                    Some(thrown)
                }
            };
            let pending = self.aim_at_always_offset(tick, ids, shot.attacker, pending);
            let pending = match (carried, pending.launch_slot) {
                (None, Some(slot)) => crate::combat::PendingProjectile {
                    from: self
                        .module_launch(tick, ids, shot.attacker, Some(slot.index()))
                        .unwrap_or(pending.from),
                    ..pending
                },
                _ => pending,
            };
            crate::perf::note_projectile_spawned();
            let projectile = tick.state_mut().spawn()?;
            let flight = Flight {
                passenger_roll,
                ..self.launch_flight(&pending, shot.attacker, carried)
            };
            let flight = if carried.is_none() {
                self.launch_object(tick, ids, flight)?
            } else {
                flight
            };
            match carried {
                Some((thrown, _)) if thrown == shot.attacker => {
                    self.launch_carried(tick, ids, shot.attacker, thrown, &flight)?;
                }
                Some((thrown, _)) => {
                    self.launch_passenger(tick, ids, shot.attacker, thrown, &flight)?;
                }
                None => {}
            }
            tick.state_mut().insert(ids.flight, projectile, flight)?;
            self.apply_weather_to_new_object(tick, ids, projectile)?;
        }
        Ok(())
    }

    fn reload_idle_weapons(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        armed: &[Entity],
        now: Frame,
    ) -> Result<()> {
        for &entity in armed {
            let Some(before) = tick.state().get(ids.armed, entity).copied() else {
                continue;
            };
            let Some(template) = self.combat.weapons.get(before.weapon) else {
                continue;
            };
            let rate = self.fire_rate(tick.state(), ids, entity, template);
            let mut after = before;
            if after
                .state
                .reload_when_idle(template, now, rate, tick.rng())
            {
                diag!(
                    "shot: frame {} entity {} reloads when idle",
                    now.get(),
                    entity.index()
                );
            }
            if after != before {
                tick.state_mut().insert(ids.armed, entity, after)?;
            }
        }
        Ok(())
    }

    fn dual_weapon_due(entity: Entity, now: Frame) -> bool {
        now.get() % u64::from(mint_sim::SIMULATION_HZ)
            == u64::from(entity.index() % mint_sim::SIMULATION_HZ)
    }

    fn advance_dual_weapons(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
        global_max_radius: f32,
        now: Frame,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let mut shooters: Vec<Entity> = tick
            .state()
            .store(wiring.dual_weapon_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        shooters.sort_unstable_by_key(|entity| entity.index());
        shooters.dedup();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        let mut candidate_indices: Vec<u32> = Vec::new();
        for entity in shooters {
            if !Self::dual_weapon_due(entity, now) {
                continue;
            }
            if tick.state().contains(ids.using_ability, entity)
                || tick.state().contains(ids.contained, entity)
                || self.is_rampaging(tick.state(), entity)
            {
                continue;
            }
            let Some(module) = tick
                .state()
                .get(wiring.dual_weapon_state, entity)
                .and_then(|column| column.0.first())
                .map(|slot| slot.state)
            else {
                continue;
            };
            if module.switch_distance <= 0.0 {
                continue;
            }
            let Some(me) = tick
                .state()
                .get(ids.body, entity)
                .map(|body| body.0.position)
            else {
                continue;
            };
            let me_z = Self::feet(tick.state(), ids, entity);
            let height_gate = module.switch_distance * 0.5;
            let within = |at: Vec2, z: f32| {
                let (dx, dy) = (at.x - me.x, at.y - me.y);
                dx * dx + dy * dy <= module.switch_distance * module.switch_distance
                    && (z - me_z).abs() <= height_gate
            };
            let close = if module.real_victim_range {
                tick.state()
                    .get(ids.armed, entity)
                    .and_then(|armed| armed.target)
                    .is_some_and(|victim| {
                        tick.state().get(ids.body, victim).is_some_and(|body| {
                            within(body.0.position, Self::feet(tick.state(), ids, victim))
                        })
                    })
            } else {
                let Some(_) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
                    continue;
                };
                let search_radius =
                    TargetGrid::search_radius(module.switch_distance, 0.0, global_max_radius);
                candidate_indices.clear();
                grid.query_into(
                    grid_cell(me.x),
                    grid_cell(me.y),
                    grid_radius_cells(search_radius),
                    &mut candidate_indices,
                );
                candidate_indices.iter().any(|&at| {
                    #[allow(clippy::cast_possible_truncation)]
                    let at = at as usize;
                    living.get(at).is_some_and(|(foe, _)| {
                        foe.entity != entity
                            && self.object_hostile(tick.state(), entity, foe.entity)
                            && within(foe.position, foe.z)
                    })
                })
            };
            let raised = tick.state().get(wiring.state, entity).is_some_and(|state| {
                state
                    .weapon_set_flags
                    .iter()
                    .any(|flag| flag.eq_ignore_ascii_case(CLOSE_RANGE))
            });
            if close == raised {
                continue;
            }
            crate::diag::diag!(
                "weapon: {} CLOSE_RANGE {}",
                entity.index(),
                if close { "on" } else { "off" },
            );
            let condition =
                mint_sage_modules::effects::names(vec!["WEAPONSTATE_CLOSE_RANGE".to_owned()]);
            if close {
                effects.push((entity, Effect::WeaponSetFlag(CLOSE_RANGE.into())));
                effects.push((
                    entity,
                    Effect::ModelConditions {
                        add: condition,
                        remove: mint_sage_modules::effects::names(vec![]),
                    },
                ));
            } else {
                effects.push((entity, Effect::ClearWeaponSetFlag(CLOSE_RANGE.into())));
                effects.push((
                    entity,
                    Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(vec![]),
                        remove: condition,
                    },
                ));
            }
        }
        self.apply_effects(tick, ids, effects)
    }

    pub(super) fn launch_of(
        &self,
        world: &World,
        entity: Entity,
        me: &Foe,
    ) -> mint_sage_combat::Point3 {
        self.launch_from(world, entity, me, 0, None)
    }

    pub(super) fn take_launch(
        &self,
        tick: &mut Tick<'_>,
        entity: Entity,
        me: &Foe,
        slot: Option<usize>,
    ) -> mint_sage_combat::Point3 {
        let count = self.barrels_in(tick.state(), entity, slot).len();
        let barrel = if count > 1 {
            let drawn = tick.rng().below(u32::try_from(count).unwrap_or(u32::MAX));
            usize::try_from(drawn).unwrap_or(0)
        } else {
            0
        };
        let from = self.launch_from(tick.state(), entity, me, barrel, slot);
        crate::diag::diag!(
            "shot: entity {} leaves from ({:.1}, {:.1}), {:.1} above its feet",
            entity.index(),
            from.x - me.position.x,
            from.y - me.position.y,
            from.z - me.z
        );
        from
    }

    pub(super) fn barrels_of(&self, world: &World, entity: Entity) -> &[crate::perches::Perch] {
        self.barrels_in(world, entity, None)
    }

    fn barrels_in(
        &self,
        world: &World,
        entity: Entity,
        slot: Option<usize>,
    ) -> &[crate::perches::Perch] {
        let Some(ids) = self.ids else {
            return &[];
        };
        let slot = slot.unwrap_or_else(|| {
            world
                .get(ids.weapon_slots, entity)
                .map_or(0, |slots| usize::from(slots.current))
        });
        let Some(launch) = world
            .get(ids.kind, entity)
            .and_then(|kind| self.launch_bones.get(usize::from(kind.0)))
            .and_then(|slots| slots.get(slot))
        else {
            return &[];
        };
        if launch.poses.is_empty() {
            return &launch.rest;
        }
        let mut held: Vec<String> = self
            .modules
            .as_ref()
            .and_then(|wiring| world.get(wiring.state, entity))
            .map(|state| state.model_condition_names())
            .unwrap_or_default();
        held.push("ATTACKING".to_owned());
        let moving = world.get(ids.body, entity).is_some_and(|body| {
            crate::render_state::is_moving(world.contains(ids.path, entity), body.0.speed)
        });
        if moving {
            held.push("MOVING".to_owned());
        }
        launch.bones(|token| held.iter().any(|name| name.eq_ignore_ascii_case(token)))
    }

    fn launch_from(
        &self,
        world: &World,
        entity: Entity,
        me: &Foe,
        barrel: usize,
        slot: Option<usize>,
    ) -> mint_sage_combat::Point3 {
        let enclosed = self
            .ids
            .and_then(|ids| self.contained_status(world, ids, entity))
            .is_some_and(|status| status.has(super::contain::ContainStatus::ENCLOSED));
        if enclosed {
            return launch_point(me.position, me.z);
        }
        let facing = self
            .ids
            .and_then(|ids| world.get(ids.body, entity))
            .map(|body| body.0.facing);
        match (self.barrels_in(world, entity, slot).get(barrel), facing) {
            (Some(bone), Some(facing)) => {
                let at = crate::templates::rotate(
                    me.position,
                    facing,
                    [bone.position[0], bone.position[1]],
                );
                mint_sage_combat::Point3::new(at.x, at.y, me.z + bone.position[2])
            }
            _ => launch_point(me.position, me.z),
        }
    }

    fn aim_of(
        &self,
        tick: &Tick<'_>,
        entity: Entity,
        weapon: WeaponId,
        me: &Foe,
        target: &Foe,
    ) -> Option<(
        mint_sage_modules::AimPitch,
        Option<mint_sage_modules::AimDistance>,
    )> {
        let wiring = self.modules.as_ref()?;
        if !self.combat.weapons.get(weapon)?.aiming {
            return None;
        }
        let module = tick.state().get(wiring.aim_state, entity)?.0.first()?.state;
        let (dx, dy) = (
            target.position.x - me.position.x,
            target.position.y - me.position.y,
        );
        let distance = (dx * dx + dy * dy).sqrt();
        let rise = (target.z + target.height) - self.launch_of(tick.state(), entity, me).z;
        let floored = distance.max(10.0);
        let slope = rise / floored;
        Some((module.pitch(slope), module.distance(floored)))
    }

    pub(super) fn aim_body(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        weapon: WeaponId,
        from: Vec2,
        victim: Vec2,
    ) -> Result<bool> {
        let Some(template) = self.combat.weapons.get(weapon) else {
            return Ok(true);
        };
        let riding = self.contained_status(tick.state(), ids, entity).is_some();
        let turns_inside = self.turns_inside(tick.state(), ids, entity);
        let swoop_pass = self.is_giant_bird(tick.state(), ids, entity);
        if swoop_pass || (riding && !turns_inside) {
            return Ok(true);
        }
        if self.turret_carries_current_weapon(tick, ids, entity) {
            return Ok(tick
                .state()
                .get(ids.turret_state, entity)
                .is_some_and(|state| state.aimed));
        }
        let contact = template.is_contact();
        let offset = victim.minus(from);
        let Some(bearing) = Facing::from_vector(offset) else {
            return Ok(true);
        };
        let towards = Facing::from_radians(bearing.radians() - template.aim_direction);
        let aim_point = from.plus(towards.to_vector().scale(offset.length()));
        let tolerance = template.aim_tolerance();
        let aimed = |off: f32| off < tolerance;
        let facing = |tick: &Tick<'_>| tick.state().get(ids.body, entity).map(|b| b.0.facing);
        let Some(before) = facing(tick) else {
            return Ok(true);
        };
        if aimed(before.delta_to(towards).abs()) {
            return Ok(true);
        }
        if !turns_inside && Self::is_immobile(tick, ids, entity) {
            return Ok(contact);
        }
        if !contact && !turns_inside && self.cannot_stop(tick, ids, entity) {
            let steer = Path {
                route: Route::to_point(aim_point),
                goal: aim_point,
                kind: PathKind::Closing,
                facing: None,
            };
            crate::unit::give_path(tick.state_mut(), (ids.body, ids.path), entity, steer)?;
            return Ok(false);
        }
        if contact && self.cannot_stop(tick, ids, entity) {
            return Ok(true);
        }
        let quick_member = tick
            .state()
            .get(ids.member, entity)
            .map(|member| member.horde)
            .filter(|_| !tick.state().contains(ids.path, entity));
        if let Some(horde) = quick_member {
            #[allow(clippy::cast_precision_loss)]
            let frames_left = crate::horde::decision_stagger(Self::tick_seconds())
                .frames_until_due(mint_movement::MoverId(horde.index()), tick.frame().get())
                as f32;
            if let Some(mut body) = tick.state().get(ids.body, entity).copied() {
                let turn = before.delta_to(towards).abs() / frames_left.max(1.0);
                body.0.facing = before.turned_towards(towards, turn);
                tick.state_mut().insert(ids.body, entity, body)?;
            }
        } else {
            self.turn_towards(tick, ids, entity, aim_point)?;
        }
        Ok(contact || facing(tick).is_some_and(|after| aimed(after.delta_to(towards).abs())))
    }

    fn keep_facing(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        weapon: WeaponId,
        from: Vec2,
        victim: Vec2,
    ) -> Result<()> {
        let Some(template) = self
            .combat
            .weapons
            .get(weapon)
            .filter(|template| template.aim_delta < core::f32::consts::PI)
        else {
            return Ok(());
        };
        let riding = self.contained_status(tick.state(), ids, entity).is_some();
        let turns_inside = self.turns_inside(tick.state(), ids, entity);
        if self.is_giant_bird(tick.state(), ids, entity)
            || (riding && !turns_inside)
            || self.turret_carries_current_weapon(tick, ids, entity)
            || (!turns_inside && Self::is_immobile(tick, ids, entity))
        {
            return Ok(());
        }
        let Some(bearing) = Facing::from_vector(victim.minus(from)) else {
            return Ok(());
        };
        let towards = Facing::from_radians(bearing.radians() - template.aim_direction);
        self.turn_towards(tick, ids, entity, from.plus(towards.to_vector()))
    }

    fn moving_blocks_fire(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        weapon: WeaponId,
    ) -> bool {
        let Some(template) = self.combat.weapons.get(weapon) else {
            return false;
        };
        if let Some(held) = tick.state().get(ids.contained, entity) {
            return self.container_moving(tick, ids, held.container)
                && !self.carried_fire_permitted(tick, ids, entity, template.can_fire_while_moving);
        }
        if template.melee || template.can_fire_while_moving || Self::is_immobile(tick, ids, entity)
        {
            return false;
        }
        tick.state().contains(ids.path, entity)
            || tick
                .state()
                .get(ids.body, entity)
                .is_some_and(|body| body.0.speed > 0.0)
    }

    fn container_moving(&self, tick: &Tick<'_>, ids: Ids, container: Entity) -> bool {
        tick.state().contains(ids.path, container)
            || tick
                .state()
                .get(ids.body, container)
                .is_some_and(|body| body.0.speed > 0.0)
    }

    fn carried_fire_permitted(&self, tick: &Tick<'_>, ids: Ids, entity: Entity, own: bool) -> bool {
        let horde_weapon = tick
            .state()
            .get(ids.member, entity)
            .and_then(|member| self.selected_set(tick, ids, member.horde))
            .and_then(|(_, slots)| slots.first_weapon())
            .and_then(|(_, weapon)| self.combat.weapons.get(weapon));
        horde_weapon.map_or(own, |template| template.can_fire_while_moving)
    }

    fn fires_at_position(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        weapon: WeaponId,
        victim: Entity,
    ) -> bool {
        self.combat
            .weapons
            .get(weapon)
            .is_some_and(|template| template.no_victim_needed)
            && !Self::is_immobile(tick, ids, victim)
    }

    fn wind_up_deferred(&self, tick: &Tick<'_>, ids: Ids, entity: Entity, state: &Armed) -> bool {
        let mut clock = state.state;
        if state.winding
            || clock.status(tick.frame()) != mint_sage_combat::WeaponStatus::ReadyToFire
            || !tick.state().contains(ids.member, entity)
        {
            return false;
        }
        let period = u64::from(crate::horde::decision_stagger(Self::tick_seconds()).period());
        crate::combat::on_odd_retail_frame(tick.frame().get(), period)
    }

    #[allow(clippy::too_many_arguments)]
    fn continue_position_shot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        mut state: Armed,
        at: Vec2,
        me: &Foe,
        living: &[(Foe, PlayerId)],
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        let rate = tick
            .state()
            .get(ids.bonuses, entity)
            .map_or(1.0, |bonuses| bonuses.0.rate_of_fire);
        let shot = tick_weapon(
            &self.combat,
            &mut state,
            tick.frame(),
            false,
            me.position,
            rate,
            Self::bonus_conditions(tick.state(), ids, entity),
            tick.rng(),
        );
        if shot.fired() {
            state.position_shot = None;
        }
        let weapon = state.weapon;
        tick.state_mut().insert(ids.armed, entity, state)?;
        if !shot.fired() {
            return Ok(());
        }
        if let Some(effect) = self.after_shot_lock(tick, ids, entity, weapon) {
            self.apply_effects(tick, ids, vec![(entity, effect)])?;
        }
        self.note_horde_last_shot(tick, ids, entity);
        self.fire_at_position(tick, ids, entity, weapon, me, at, living, events)
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_at_position(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        weapon: WeaponId,
        me: &Foe,
        at: Vec2,
        living: &[(Foe, PlayerId)],
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        if self.fire_linear_volley(tick, ids, entity, weapon, me, living, events)? {
            return Ok(());
        }
        self.fire_ground_shot(tick, ids, entity, weapon, me, at, living, events)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fire_linear_volley(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        weapon: WeaponId,
        me: &Foe,
        living: &[(Foe, PlayerId)],
        events: &mut Vec<DamageEvent>,
    ) -> Result<bool> {
        let Some(template) = self.combat.weapons.get(weapon) else {
            return Ok(false);
        };
        let points = &template.linear_targets;
        if points.is_empty() {
            return Ok(false);
        }
        let facing = tick
            .state()
            .get(ids.body, entity)
            .map_or(0.0, |body| body.0.facing.radians());
        let (sin, cos) = (mint_math::f32::sin(facing), mint_math::f32::cos(facing));
        for _ in 0..points.len() {
            let Some(at) = tick
                .state_mut()
                .get_mut(ids.armed, entity)
                .and_then(|armed| armed.state.take_linear_index(points.len()))
            else {
                break;
            };
            let Some(point) = points.get(at) else {
                break;
            };
            if !(template.ignore_linear_first_target && at == 0) {
                let landing = Vec2::new(
                    me.position.x + point.x * cos - point.y * sin,
                    me.position.y + point.x * sin + point.y * cos,
                );
                self.fire_ground_shot(tick, ids, entity, weapon, me, landing, living, events)?;
            }
            if point.ends_volley {
                break;
            }
        }
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_ground_shot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        weapon: WeaponId,
        me: &Foe,
        at: Vec2,
        living: &[(Foe, PlayerId)],
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        let Some(template) = self.combat.weapons.get(weapon) else {
            return Ok(());
        };
        let landing = match template.resolve_shot(tick.rng(), false, false) {
            ShotOutcome::Struck => at,
            ShotOutcome::Missed([dx, dy]) => Vec2::new(at.x + dx, at.y + dy),
        };
        let shot = Shot::at_ground(
            weapon,
            entity,
            self.take_launch(tick, entity, me, None),
            landing,
            self.ground_height(landing.x, landing.y),
        );
        self.release_shot(tick, ids, &shot, entity, landing, None, living, events)
    }

    pub(super) fn abort_wind_up(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let Some(mut armed) = tick.state().get(ids.armed, entity).copied() else {
            return Ok(());
        };
        if !armed.winding {
            return Ok(());
        }
        armed.winding = false;
        armed.state.abort_wind_up();
        tick.state_mut().insert(ids.armed, entity, armed)?;
        Ok(())
    }

    fn current_slot(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        weapon: WeaponId,
    ) -> Option<u8> {
        if let Some(held) = tick.state().get(ids.weapon_slots, entity) {
            return Some(held.current);
        }
        let (_, slots) = self.selected_set(tick, ids, entity)?;
        let at = slots
            .arms
            .iter()
            .position(|arm| arm.is_some_and(|arm| arm.weapon == weapon))?;
        u8::try_from(at).ok()
    }

    fn lock_when_using(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        weapon: WeaponId,
    ) -> Option<Effect> {
        if !self.combat.weapons.get(weapon)?.lock_when_using {
            return None;
        }
        let slot = self.current_slot(tick, ids, entity, weapon)?;
        Some(Effect::LockWeaponSlotTemporarily(slot))
    }

    fn after_shot_lock(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        weapon: WeaponId,
    ) -> Option<Effect> {
        let template = self.combat.weapons.get(weapon)?;
        if template.idle_after_firing.is_none() {
            return self
                .holds_temporary_weapon_lock(tick, entity)
                .then_some(Effect::ReleaseTemporaryWeaponLock);
        }
        self.lock_when_using(tick, ids, entity, weapon)
    }

    fn finishing_victim(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &Armed,
        living: &[(Foe, PlayerId)],
    ) -> Option<Foe> {
        if !state.winding
            || !self
                .combat
                .weapons
                .get(state.weapon)
                .is_some_and(|template| template.finish_attack_once_started)
        {
            return None;
        }
        let victim = state.target?;
        if living
            .binary_search_by_key(&victim, |(foe, _)| foe.entity)
            .is_ok()
            || !tick
                .state()
                .get(ids.health, victim)
                .is_some_and(|health| health.0.is_dead())
        {
            return None;
        }
        self.foe_unchecked(tick, ids, victim)
    }

    #[allow(clippy::too_many_arguments)]
    fn continued_victim(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &Armed,
        shooter: Entity,
        victim: Entity,
        _: PlayerId,
        anti_mask: mint_sage_combat::AntiMask,
        living: &[(Foe, PlayerId)],
    ) -> Option<Entity> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("continued_victim"));
        let range = self.combat.weapons.get(state.weapon)?.continue_attack_range;
        if range <= 0.0
            || living
                .binary_search_by_key(&victim, |(foe, _)| foe.entity)
                .is_ok()
        {
            return None;
        }
        let at = tick.state().get(ids.body, victim)?.0.position;
        let owner = tick.state().get(ids.owner, victim)?.0;
        if !self.object_hostile(tick.state(), shooter, victim) {
            return None;
        }
        let shooter_off_map = self.scanner_off_map(tick.state(), ids, shooter);
        living
            .iter()
            .filter(|(foe, player)| {
                *player == owner
                    && self.off_map(tick.state(), ids, foe.position) == shooter_off_map
                    && !foe.unattackable
                    && target_allowed(anti_mask, foe)
                    && foe.position.minus(at).length_squared() <= range * range
            })
            .min_by(|(a, _), (b, _)| {
                let da = a.position.minus(at).length_squared();
                let db = b.position.minus(at).length_squared();
                da.total_cmp(&db)
            })
            .map(|(foe, _)| foe.entity)
    }

    fn rebuild_attack(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity, state: &mut Armed) {
        if tick
            .state()
            .get(ids.path, entity)
            .is_some_and(|path| path.kind == PathKind::Closing)
        {
            tick.state_mut().remove(ids.path, entity);
        }
        self.end_loose_attack(tick, ids, entity);
        state.winding = false;
    }

    fn fire_view_blocked(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
    ) -> bool {
        victim.airborne.is_none()
            && self.airborne_category(tick.state(), ids, entity).is_none()
            && self.needs_line_of_sight(tick, ids, entity)
            && (self.view_blocked(tick, ids, me.position, victim.position, victim.radius)
                || self.terrain_blocks_fire(tick, ids, entity, me, victim))
    }

    pub(super) fn view_blocked(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        from: Vec2,
        victim: Vec2,
        victim_radius: f32,
    ) -> bool {
        use mint_movement::Passability as _;
        let toward = from.minus(victim);
        let distance = toward.length();
        let skip = victim_radius + self.passability.geometry.cell_size();
        if distance <= skip {
            return false;
        }
        let edge = victim.plus(toward.scale(skip / distance));
        let (Some(start), Some(end)) = (
            self.passability.cell_at(from),
            self.passability.cell_at(edge),
        ) else {
            return false;
        };
        let view = self.dynamic_view(tick, ids);
        if !view.is_passable(start, SIGHT) {
            return false;
        }
        !mint_movement::smoothing::line_is_passable(&view, SIGHT, start, end)
    }

    fn turret_carries_current_weapon(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let Some(turret) = self
            .ai_table_of(tick, ids, entity)
            .and_then(|ai| ai.turret.as_ref())
        else {
            return false;
        };
        let current = tick
            .state()
            .get(ids.weapon_slots, entity)
            .map_or(0, |slots| usize::from(slots.current));
        turret.turn_rate != Some(0.0)
            && turret
                .controlled_weapon_slots
                .iter()
                .filter_map(|name| mint_sage_combat::WeaponSlot::parse(name))
                .any(|slot| slot.index() == current)
    }

    fn advance_turrets(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        armed: &[Entity],
        formations_on: bool,
        now: Frame,
    ) -> Result<()> {
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for &entity in armed {
            let Some(turret) = self
                .ai_table_of(tick, ids, entity)
                .and_then(|ai| ai.turret.clone())
            else {
                continue;
            };
            if turret.turn_rate == Some(0.0) {
                continue;
            }
            let Some(body) = tick.state().get(ids.body, entity).map(|b| b.0) else {
                continue;
            };
            let natural = body.facing;
            let desired = self
                .turret_aim_point(tick, ids, entity, formations_on)
                .and_then(|point| Facing::from_vector(point.minus(body.position)));
            let previous = tick.state().get(ids.turret_state, entity).copied();
            let heading = previous.map_or(natural, |state| state.heading);
            #[allow(clippy::cast_precision_loss)]
            let step = turret
                .turn_rate
                .map_or(DEFAULT_TURRET_TURN_RATE_RAD_PER_SEC, f32::to_radians)
                / mint_sim::SIMULATION_HZ as f32;
            let (next, goal, last_aim_frame) = match desired {
                Some(target) => {
                    let target = Self::turret_deflection_clamp(&turret, natural, target);
                    (heading.turned_towards(target, step), target, now)
                }
                None => {
                    let held_since = previous.map_or(now, |state| state.last_aim_frame);
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let recenter_ms = turret
                        .recenter_time_ms
                        .map_or(TURRET_RECENTER_DEFAULT_MS, |ms| ms.max(0.0) as u32);
                    let recenter_after = mint_sage_modules::retail_duration_ticks(recenter_ms);
                    let due = now
                        .since(held_since)
                        .is_some_and(|elapsed| elapsed >= recenter_after);
                    let next = if due {
                        heading.turned_towards(natural, step * TURRET_RECENTER_RATE_SCALE)
                    } else {
                        heading
                    };
                    (next, natural, held_since)
                }
            };
            let rotating = next.delta_to(goal).abs() > TURRET_AIM_TOLERANCE;
            let aimed = desired.is_some() && !rotating;
            tick.state_mut().insert(
                ids.turret_state,
                entity,
                TurretState {
                    heading: next,
                    last_aim_frame,
                    aimed,
                },
            )?;
            if rotating != self.holds_model_condition(tick, entity, TURRET_ROTATE) {
                effects.push((
                    entity,
                    if rotating {
                        Effect::ModelConditions {
                            add: mint_sage_modules::effects::names(vec![TURRET_ROTATE.to_owned()]),
                            remove: mint_sage_modules::effects::names(vec![]),
                        }
                    } else {
                        Effect::ModelConditions {
                            add: mint_sage_modules::effects::names(vec![]),
                            remove: mint_sage_modules::effects::names(vec![
                                TURRET_ROTATE.to_owned()
                            ]),
                        }
                    },
                ));
            }
        }
        self.apply_effects(tick, ids, effects)
    }

    fn turret_deflection_clamp(
        turret: &mint_sage_modules::TurretDef,
        natural: Facing,
        desired: Facing,
    ) -> Facing {
        let cw = turret
            .max_deflection_cw
            .unwrap_or(core::f32::consts::PI)
            .max(0.0);
        let acw = turret
            .max_deflection_acw
            .unwrap_or(core::f32::consts::PI)
            .max(0.0);
        if cw + acw >= core::f32::consts::TAU {
            return desired;
        }
        let offset = natural.delta_to(desired);
        if offset < -cw || offset > acw {
            natural
        } else {
            desired
        }
    }

    fn turret_aim_point(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        formations_on: bool,
    ) -> Option<Vec2> {
        if let Some(order) = self.ground_order_of(tick, ids, entity, formations_on) {
            return Some(order.at);
        }
        let victim = tick
            .state()
            .get(ids.attack_order, entity)
            .map(|order| order.target)
            .or_else(|| {
                tick.state()
                    .get(ids.armed, entity)
                    .and_then(|armed| armed.target)
            })?;
        tick.state()
            .get(ids.body, victim)
            .map(|body| body.0.position)
    }

    pub(super) fn holds_model_condition(
        &self,
        tick: &Tick<'_>,
        entity: Entity,
        name: &str,
    ) -> bool {
        self.modules.as_ref().is_some_and(|wiring| {
            tick.state().get(wiring.state, entity).is_some_and(|state| {
                state
                    .model_conditions
                    .iter()
                    .any(|held| held.eq_ignore_ascii_case(name))
            })
        })
    }

    fn aim_cleared(&self, tick: &Tick<'_>, entity: Entity) -> Option<Effect> {
        let wiring = self.modules.as_ref()?;
        tick.state().get(wiring.aim_state, entity)?.0.first()?;
        Some(aim_condition(None, None))
    }

    fn give_up_chase(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        target: Entity,
        now: Frame,
    ) -> bool {
        let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
            return false;
        };
        if !self.chase_leash_spent(tick, ids, entity, owner, target) {
            return false;
        }
        if tick
            .state()
            .get(ids.path, entity)
            .is_some_and(|path| path.kind == PathKind::Closing)
        {
            tick.state_mut().remove(ids.path, entity);
        }
        crate::diag::diag!(
            "chase: entity {} gives up on {} at frame {}, beyond its StopChaseDistance",
            entity.index(),
            target.index(),
            now.get(),
        );
        true
    }

    pub(super) fn needs_line_of_sight(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        self.has_kind_bit(tick.state(), ids, entity, ATTACK_NEEDS_LINE_OF_SIGHT)
            || self.has_kind_bit(tick.state(), ids, entity, CAN_SHOOT_OVER_WALLS)
    }

    fn ranged_close_goal(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        ranges: (f32, f32),
        budgets: &mut PathBudgets,
    ) -> (Vec2, f32, bool) {
        let too_close = stand_off(
            me.position,
            victim.position,
            me.radius + victim.radius,
            ranges,
        );
        let straight = too_close.map_or((victim.position, victim.radius), |spot| (spot, 0.0));
        if self.fleeing(tick, ids, me.position, victim.entity) {
            return (straight.0, straight.1, false);
        }
        if let Some(at) = self.attack_shortcut(
            tick,
            ids,
            entity,
            (me.position, me.radius),
            victim.reach_volume(),
            ranges,
            |_| true,
        ) {
            return (at, 0.0, true);
        }
        let search = if self.needs_line_of_sight(tick, ids, entity) {
            AttackSpot::InRangeInView
        } else {
            AttackSpot::InRange
        };
        let (goal, reach) = self.los_search_goal(
            tick,
            ids,
            entity,
            search,
            me.position,
            me.radius,
            victim.position,
            victim.radius,
            victim.volume,
            Some((entity, victim.z + victim.height * 0.5)),
            ranges,
            budgets,
            straight,
        );
        (goal, reach, true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn los_search_goal(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        mover: Entity,
        search: AttackSpot,
        me_position: Vec2,
        me_radius: f32,
        victim_position: Vec2,
        victim_radius: f32,
        victim_box: Option<mint_sage_combat::BoxVolume>,
        aim: Option<(Entity, f32)>,
        (min_range, max_range): (f32, f32),
        budgets: &mut PathBudgets,
        straight: (Vec2, f32),
    ) -> (Vec2, f32) {
        let _perf = crate::perf::sub(crate::perf::sub_row!("los_search_goal"));
        if search == AttackSpot::Straight {
            return straight;
        }
        let mover_class = self.mover_class(tick, ids, mover);
        let (Some(start), Some(victim_cell)) = (
            self.passable_cell_for(tick, ids, me_position, mover_class),
            self.passable_cell_near(tick, ids, victim_position, victim_radius, mover_class),
        ) else {
            return straight;
        };
        let view = self.dynamic_view(tick, ids);
        let geometry = self.passability.geometry;
        #[allow(clippy::cast_precision_loss)]
        let cost_per_unit = mint_movement::COST_ORTHOGONAL as f32 / geometry.cell_size();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let floor = ((max_range * 0.5).max(0.0) * cost_per_unit) as u32;
        let terrain =
            aim.and_then(|(shooter, z)| Some((self.terrain_sight_height(tick, ids, shooter)?, z)));
        let serves = |cell| {
            crate::combat::in_range_volumes(
                (geometry.centre_of(cell), me_radius, None),
                (victim_position, victim_radius, victim_box),
                0.0,
                min_range,
                max_range,
            ) && (search == AttackSpot::InRange
                || (!self.view_blocked(
                    tick,
                    ids,
                    geometry.centre_of(cell),
                    victim_position,
                    victim_radius,
                ) && !terrain.is_some_and(|(top, aim_z)| {
                    let at = geometry.centre_of(cell);
                    let feet = self
                        .passability
                        .heights
                        .height_at(at.x, at.y)
                        .unwrap_or(0.0);
                    self.terrain_blocked(tick, ids, top, (at, feet), (victim_position, aim_z))
                })))
        };
        if !budgets.is_head(mover) {
            let planned = tick.state().get(ids.path, mover);
            if let Some(cell) = planned.and_then(|path| self.passability.cell_at(path.goal)) {
                if serves(cell) {
                    return (geometry.centre_of(cell), 0.0);
                }
            }
        }
        let budget = budgets.budget_for(mover);
        let mut fresh;
        let mut kept = self.attack_spot_search.try_borrow_mut();
        let search = match kept.as_deref_mut() {
            Ok(kept) => kept,
            Err(_) => {
                fresh = mint_movement::AStar::octile();
                &mut fresh
            }
        };
        let outcome = search.find_where(
            &view,
            start,
            mover_class,
            victim_cell,
            floor,
            budget,
            serves,
        );
        match outcome {
            Ok(mint_movement::SearchResult {
                outcome: mint_movement::SearchOutcome::Found(path),
                ..
            }) => match path.cells.last() {
                Some(&cell) => (geometry.centre_of(cell), 0.0),
                None => straight,
            },
            _ => straight,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ranged_approach_step(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        ranges: (f32, f32),
        budgets: &mut PathBudgets,
    ) -> Result<bool> {
        let now = tick.frame();
        if let Some(wait) = tick
            .state()
            .get(ids.approach_wait, entity)
            .copied()
            .filter(|wait| wait.target == victim.entity)
        {
            if now < wait.until {
                return Ok(false);
            }
            tick.state_mut().remove(ids.approach_wait, entity);
            crate::diag::diag!(
                "approach: frame {} entity {} gives up waiting on {}",
                now.get(),
                entity.index(),
                victim.entity.index(),
            );
            return Ok(true);
        }
        if self.cannot_move_at_all(tick, ids, entity) {
            self.set_approach_wait(tick, ids, entity, victim.entity, APPROACH_WAIT_CANNOT_MOVE)?;
            return Ok(false);
        }
        if self.stand_ground_requires_wait(tick, ids, entity, me, victim) {
            let wait = if tick.state().get(ids.path, entity).is_some() {
                APPROACH_WAIT_NO_ROUTE
            } else {
                APPROACH_WAIT_CANNOT_MOVE
            };
            self.set_approach_wait(tick, ids, entity, victim.entity, wait)?;
            return Ok(false);
        }
        let needs_los = self.needs_line_of_sight(tick, ids, entity);
        let can_go = !needs_los || self.can_approach_to_target(tick, ids, entity, me, victim);
        if can_go {
            if tick.state().get(ids.path_retry, entity).is_some()
                && !self.can_approach_to_target(tick, ids, entity, me, victim)
            {
                self.set_approach_wait(tick, ids, entity, victim.entity, APPROACH_WAIT_NO_ROUTE)?;
                return Ok(false);
            }
            if !self.fleeing(tick, ids, me.position, victim.entity)
                && crate::combat::in_range_volumes(
                    me.reach_volume(),
                    victim.reach_volume(),
                    0.0,
                    ranges.0,
                    ranges.1,
                )
                && (victim.airborne.is_some()
                    || self.airborne_category(tick.state(), ids, entity).is_some()
                    || !(self.view_blocked(tick, ids, me.position, victim.position, victim.radius)
                        || self.terrain_blocks_fire(tick, ids, entity, me, victim)))
            {
                if tick
                    .state()
                    .get(ids.path, entity)
                    .is_some_and(|path| path.kind == PathKind::Closing)
                {
                    tick.state_mut().remove(ids.path, entity);
                }
                return Ok(false);
            }
            let (goal, goal_reach, attack_path) =
                self.ranged_close_goal(tick, ids, entity, me, victim, ranges, budgets);
            let spot = attack_path && goal_reach == 0.0;
            if spot {
                self.close_on_attack_spot(tick, ids, entity, me.position, goal, budgets)?;
            } else {
                self.close_on_target(
                    tick,
                    ids,
                    entity,
                    me.position,
                    goal,
                    goal_reach,
                    PathKind::Closing,
                    budgets,
                )?;
            }
            self.note_ranged_plan(
                tick,
                ids,
                entity,
                goal,
                (victim.entity, victim.position),
                spot,
            )?;
            return Ok(false);
        }
        if let Some(spot) = self.engage_fallback_point(tick, ids, me, victim) {
            self.close_on_target(
                tick,
                ids,
                entity,
                me.position,
                spot,
                0.0,
                PathKind::Closing,
                budgets,
            )?;
            return Ok(false);
        }
        self.set_approach_wait(tick, ids, entity, victim.entity, APPROACH_WAIT_NO_ROUTE)?;
        Ok(false)
    }

    fn cannot_move_at_all(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        self.move_refused(tick, ids, entity).is_some()
            || self.mover_limits(tick, ids, entity).max_speed < APPROACH_CANNOT_MOVE_SPEED
    }

    fn stand_ground(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        self.ai_table_of(tick, ids, entity)
            .is_some_and(|ai| ai.stand_ground)
    }

    fn victim_on_wall_layer(&self, _tick: &Tick<'_>, _ids: Ids, _victim: Entity) -> bool {
        #[cfg(test)]
        if FORCE_VICTIM_ON_WALL_LAYER.with(std::cell::Cell::get) {
            return true;
        }
        false
    }

    fn stand_ground_requires_wait(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
    ) -> bool {
        self.stand_ground(tick, ids, entity)
            && self.victim_on_wall_layer(tick, ids, victim.entity)
            && !self.can_approach_to_target(tick, ids, entity, me, victim)
    }

    fn set_approach_wait(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        target: Entity,
        wait: LogicFrames,
    ) -> Result<()> {
        let until = tick.frame().checked_add(wait).unwrap_or(Frame::FOREVER);
        tick.state_mut()
            .insert(ids.approach_wait, entity, ApproachWait { until, target })?;
        Ok(())
    }

    fn can_approach_to_target(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
    ) -> bool {
        let mover = self.mover_class(tick, ids, entity);
        let (Some(start), Some(goal)) = (
            self.passable_cell_for(tick, ids, me.position, mover),
            self.passable_cell_near(tick, ids, victim.position, victim.radius, mover),
        ) else {
            return false;
        };
        self.reachable(mover, start, goal)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn close_on_target(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        from: Vec2,
        to: Vec2,
        target_reach: f32,
        kind: PathKind,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        self.route_toward(
            tick,
            ids,
            entity,
            (from, to, target_reach),
            kind,
            budgets,
            RouteEnd::Reach,
            ReplanCadence::Normal,
            true,
        )
        .map(|_| ())
    }

    pub(super) fn close_on_attack_spot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        from: Vec2,
        spot: Vec2,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        self.route_toward(
            tick,
            ids,
            entity,
            (from, spot, 0.0),
            PathKind::Closing,
            budgets,
            RouteEnd::Spot,
            ReplanCadence::Normal,
            true,
        )
        .map(|_| ())
    }

    pub(super) fn walk_toward(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        from: Vec2,
        to: Vec2,
        budgets: &mut PathBudgets,
        final_goal: bool,
    ) -> Result<Option<Vec2>> {
        self.route_toward(
            tick,
            ids,
            entity,
            (from, to, 0.0),
            PathKind::Closing,
            budgets,
            RouteEnd::Closest,
            ReplanCadence::Normal,
            final_goal,
        )
    }

    pub(super) fn close_on_target_boarding(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        from: Vec2,
        to: Vec2,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        self.route_toward(
            tick,
            ids,
            entity,
            (from, to, 0.0),
            PathKind::Ordered,
            budgets,
            RouteEnd::Reach,
            ReplanCadence::Boarding,
            true,
        )
        .map(|_| ())
    }

    pub(super) fn walk_toward_ordered(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        from: Vec2,
        to: Vec2,
        budgets: &mut PathBudgets,
        final_goal: bool,
    ) -> Result<Option<Vec2>> {
        self.route_toward(
            tick,
            ids,
            entity,
            (from, to, 0.0),
            PathKind::Ordered,
            budgets,
            RouteEnd::Closest,
            ReplanCadence::Normal,
            final_goal,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn route_toward(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        (from, to, target_reach): (Vec2, Vec2, f32),
        kind: PathKind,
        budgets: &mut PathBudgets,
        end: RouteEnd,
        cadence: ReplanCadence,
        final_goal: bool,
    ) -> Result<Option<Vec2>> {
        if self.move_refused(tick, ids, entity).is_some() {
            return Ok(None);
        }
        if kind != PathKind::Ability && Self::on_portal_leg(tick.state(), ids, entity) {
            return Ok(None);
        }
        let now = tick.frame();
        let stale = tick.state().get(ids.path, entity).is_none_or(|path| {
            let due = |frames: u64| {
                tick.state()
                    .get(ids.replanned_at, entity)
                    .is_none_or(|last| now.get().saturating_sub(last.0.get()) >= frames)
            };
            match cadence {
                ReplanCadence::Normal => {
                    super::movement::goal_moved(from, path.goal, to)
                        && due(super::movement::REPLAN_COOLDOWN_FRAMES)
                }
                ReplanCadence::Boarding => {
                    from.minus(path.goal).length() > super::movement::BOARDING_REPLAN_DISTANCE
                        && due(super::movement::BOARDING_REPLAN_FRAMES)
                }
            }
        });
        let is_head = budgets.is_head(entity);
        if !stale && !is_head {
            return Ok(None);
        }
        if self.give_off_map_route(tick, ids, entity, (from, to), kind)? {
            tick.state_mut()
                .insert(ids.replanned_at, entity, super::movement::ReplannedAt(now))?;
            return Ok(None);
        }
        let budget = budgets.budget_for(entity);
        if budget.remaining() == 0 {
            return Ok(None);
        }
        let modes = self.modes(tick, ids);
        let mover = self.mover_class(tick, ids, entity);
        let closest = end == RouteEnd::Closest;
        let goal_cell = if closest {
            self.order_goal_cell(tick, ids, to, mover)
        } else {
            self.passable_cell_near(tick, ids, to, target_reach, mover)
        };
        let (Some(start), Some(goal_cell)) = (
            self.start_cell_toward(tick, ids, from, to, mover),
            goal_cell,
        ) else {
            return Ok(None);
        };
        let prune = !self.reachable(mover, start, goal_cell);
        let request = PathRequest {
            start,
            goal: goal_cell,
            mover,
            goal_id: GoalId(u64::from(entity.index())),
            origin: Some(from),
            corner_radius: self.mover_limits(tick, ids, entity).corner_radius,
            departure: self.departure(tick, ids, entity),
            prune,
            final_goal,
            approach: if kind == PathKind::Closing {
                tick.state()
                    .get(ids.body, entity)
                    .map(|body| body.0.facing.to_vector())
            } else {
                None
            },
        };
        let ground = self.dynamic_view(tick, ids);
        let costs = if kind == PathKind::Closing {
            self.attack_cost_view(tick, ids, entity, &ground)
        } else {
            self.unit_cost_view(tick, ids, entity, &ground)
        }
        .exempting(&[goal_cell]);
        let view = self.footprint_view(tick, ids, entity, mover, &costs, &ground);
        crate::perf::note_plan_call();
        let planning_error = |e| mint_sim::Error::Rules {
            frame: tick.frame().get(),
            reason: format!("planning an attack path for entity {}: {e}", entity.index()),
        };
        let site = crate::perf::sub(crate::perf::sub_row!("path_site_attack"));
        let mut outcome = self
            .plan_path(modes.config(), &view, &request, budget)
            .map_err(planning_error)?;
        drop(site);
        let failed = match outcome {
            PlanOutcome::Unreachable => true,
            PlanOutcome::BudgetExhausted => is_head,
            _ => false,
        };
        if closest && failed {
            let mut whole = NodeBudget::new(super::movement::PATH_NODE_BUDGET);
            outcome = Pipeline::new(modes.config())
                .plan_closest_accepting(
                    &view,
                    self.passability.geometry,
                    &self.stepped_back(&request),
                    &mut whole,
                    &self.footprint_accepts(tick, ids, entity),
                    if kind == PathKind::Closing {
                        APPROACH_COST_WEIGHT
                    } else {
                        0.0
                    },
                )
                .map_err(planning_error)?;
        }
        let mut short_of = None;
        let exact = matches!(outcome, PlanOutcome::Ready(_));
        match outcome {
            PlanOutcome::Ready(plan) | PlanOutcome::Closest(plan) => {
                let goal = if exact {
                    to
                } else {
                    plan.route.points.last().copied().unwrap_or(from)
                };
                let mut route = plan.route;
                if exact && end == RouteEnd::Spot && self.passability.cell_at(to) == Some(goal_cell)
                {
                    if let Some(last) = route.points.last_mut() {
                        *last = to;
                    }
                }
                if !exact {
                    short_of = Some(goal);
                }
                tick.state_mut().insert(
                    ids.replanned_at,
                    entity,
                    super::movement::ReplannedAt(now),
                )?;
                crate::diag::diag!(
                    "path: frame {} entity {} closes from ({:.0}, {:.0}) to ({:.0}, {:.0})",
                    tick.frame().get(),
                    entity.index(),
                    from.x,
                    from.y,
                    to.x,
                    to.y
                );
                Self::clear_path_retry(tick, ids, entity);
                if let Some(mut body) = tick.state().get(ids.body, entity).copied() {
                    body.0.waypoint = 0;
                    tick.state_mut().insert(ids.body, entity, body)?;
                }
                let points = route.points.clone();
                tick.state_mut().insert(
                    ids.path,
                    entity,
                    Path {
                        route,
                        goal,
                        kind,
                        facing: None,
                    },
                )?;
                let mut pipeline = Pipeline::new(modes.config());
                self.move_allies(tick, ids, &mut pipeline, entity, &points)?;
            }
            PlanOutcome::BudgetExhausted => {
                if is_head {
                    self.note_path_failure(tick, ids, entity)?;
                } else {
                    self.enqueue_path_request(tick, ids, entity)?;
                }
            }
            PlanOutcome::Unreachable => self.note_path_failure(tick, ids, entity)?,
            _ => {}
        }
        Ok(short_of)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApproachWait {
    pub(crate) until: Frame,
    pub(crate) target: Entity,
}

impl Component for ApproachWait {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.until.get());
        self.target.hash_into(hasher);
    }
}

const APPROACH_WAIT_CANNOT_MOVE: LogicFrames = LogicFrames::from_engine_milliseconds(2_000);

const APPROACH_WAIT_NO_ROUTE: LogicFrames = LogicFrames::from_engine_milliseconds(10_000);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TurretState {
    pub(crate) heading: Facing,
    pub(crate) last_aim_frame: Frame,
    pub(crate) aimed: bool,
}

impl Component for TurretState {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.heading.radians());
        self.last_aim_frame.hash_into(hasher);
        hasher.write_bool(self.aimed);
    }
}

const TURRET_AIM_TOLERANCE: f32 = 0.035;

const DEFAULT_TURRET_TURN_RATE_RAD_PER_SEC: f32 = 0.05;

const TURRET_RECENTER_DEFAULT_MS: u32 = 2_000;

const TURRET_RECENTER_RATE_SCALE: f32 = 0.5;

const TURRET_ROTATE: &str = "TURRET_ROTATE";

const APPROACH_CANNOT_MOVE_SPEED: f32 = 0.1;

const PURSUE_IN_RANGE_SPEED: f32 = 0.95;

const SIGHT: mint_movement::MoverClass =
    mint_movement::MoverClass::new(mint_sage_passability::OBSTACLE);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttackSpot {
    Straight,
    InRange,
    InRangeInView,
}

pub(super) fn stand_off(
    me: Vec2,
    target: Vec2,
    radii: f32,
    (min_range, max_range): (f32, f32),
) -> Option<Vec2> {
    if !min_range.is_finite() || !max_range.is_finite() || min_range <= 0.0 {
        return None;
    }
    if max_range <= min_range {
        return None;
    }
    let away = me.minus(target);
    let edge = min_range + radii;
    if away.length_squared() >= edge * edge {
        return None;
    }
    let direction = away.normalised().unwrap_or(Vec2::new(1.0, 0.0));
    Some(target.plus(direction.scale((min_range + max_range) * 0.5 + radii)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AfterFiring {
    Free,
    Holding,
    EndsAttack,
}

pub(super) fn after_firing(
    template: &mint_sage_combat::WeaponTemplate,
    armed: &Armed,
    now: Frame,
) -> AfterFiring {
    let Some(shot) = armed.state.last_shot() else {
        return AfterFiring::Free;
    };
    let wait = template
        .firing_duration
        .get()
        .max(template.hold_after_firing.get())
        .max(template.idle_after_firing.map_or(0, LogicFrames::get));
    if now
        < shot
            .checked_add(LogicFrames::from_frames(wait))
            .unwrap_or(Frame::FOREVER)
    {
        AfterFiring::Holding
    } else if template.idle_after_firing.is_some() && armed.shots_at_target > 0 {
        AfterFiring::EndsAttack
    } else {
        AfterFiring::Free
    }
}

fn aim_condition(
    pitch: Option<mint_sage_modules::AimPitch>,
    distance: Option<mint_sage_modules::AimDistance>,
) -> Effect {
    if pitch.is_none() && distance.is_none() {
        return Effect::ModelConditions {
            add: mint_sage_modules::effects::names(vec![]),
            remove: mint_sage_modules::effects::names(
                mint_sage_modules::AIM_CONDITIONS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            ),
        };
    }
    let mut add: Vec<String> = Vec::new();
    let mut remove: Vec<String> = Vec::new();
    if let Some(pitch) = pitch {
        add.push(pitch.condition().to_owned());
    }
    if let Some(distance) = distance {
        add.push(distance.condition().to_owned());
        remove.push(distance.other().to_owned());
    }
    Effect::ModelConditions {
        add: mint_sage_modules::effects::names(add),
        remove: mint_sage_modules::effects::names(remove),
    }
}
