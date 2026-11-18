use mint_movement::Vec2;
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{grid_cell, grid_radius_cells, Foe, TargetGrid};
use crate::defeat::ObjectFilter;
use crate::diag::diag;
use crate::templates::{AutoButton, TargetKind};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AutoCastSlot(Option<String>);

impl Component for AutoCastSlot {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match &self.0 {
            Some(power) => {
                hasher.write_bool(true);
                hasher.write_usize(power.len());
                hasher.write_bytes(power.as_bytes());
            }
            None => hasher.write_bool(false),
        }
    }
}

const SCAN_INTERVAL: LogicFrames = LogicFrames::from_frames(36);

const AUTO_CAST_DEFAULT_DELAY: LogicFrames = LogicFrames::from_frames(10 * mint_sim::SIMULATION_HZ);

const NEED_DAMAGED_TARGET_RATIO: f32 = 0.8;

const SCAN_START_CELLS: i32 = 1;

pub(super) fn nearest_by_expanding_scan(
    position: Vec2,
    grid: &TargetGrid,
    min_range: f32,
    max_range: f32,
    min_matches: u32,
    passing: impl Fn(u32) -> Option<(f32, Entity)>,
) -> Option<Option<Entity>> {
    let (cx, cy) = (grid_cell(position.x), grid_cell(position.y));
    let max_reach = max_range * max_range;
    let full_cells = grid_radius_cells(max_range).min(grid.occupied_extent_cells(cx, cy));
    let mut radius_cells = grid_radius_cells(min_range)
        .max(SCAN_START_CELLS)
        .min(full_cells);
    let mut near: Vec<u32> = Vec::new();
    loop {
        let last = radius_cells >= full_cells;
        #[allow(clippy::cast_precision_loss)]
        let world = radius_cells as f32 * crate::combat::TARGET_GRID_CELL;
        let accept = if last {
            max_reach
        } else {
            (world * world).min(max_reach)
        };
        near.clear();
        grid.query_into(cx, cy, radius_cells, &mut near);
        let mut count: u32 = 0;
        let mut nearest: Option<(f32, Entity)> = None;
        for index in &near {
            let Some((reach, foe)) = passing(*index) else {
                continue;
            };
            if reach > accept {
                continue;
            }
            count = count.saturating_add(1);
            let better = nearest.is_none_or(|(best_reach, best_entity)| {
                reach < best_reach || (reach == best_reach && foe.index() < best_entity.index())
            });
            if better {
                nearest = Some((reach, foe));
            }
        }
        if last {
            return (count >= min_matches).then(|| nearest.map(|(_, target)| target));
        }
        if min_matches > 0 && count >= min_matches {
            return Some(nearest.map(|(_, target)| target));
        }
        radius_cells = radius_cells.saturating_mul(2).min(full_cells);
    }
}

#[cfg(test)]
pub(super) fn nearest_by_full_scan(
    position: Vec2,
    grid: &TargetGrid,
    max_range: f32,
    min_matches: u32,
    passing: impl Fn(u32) -> Option<(f32, Entity)>,
) -> Option<Option<Entity>> {
    let mut near: Vec<u32> = Vec::new();
    grid.query_into(
        grid_cell(position.x),
        grid_cell(position.y),
        grid_radius_cells(max_range),
        &mut near,
    );
    let mut count: u32 = 0;
    let mut nearest: Option<(f32, Entity)> = None;
    for index in &near {
        let Some((reach, foe)) = passing(*index) else {
            continue;
        };
        count = count.saturating_add(1);
        let better = nearest.is_none_or(|(best_reach, best_entity)| {
            reach < best_reach || (reach == best_reach && foe.index() < best_entity.index())
        });
        if better {
            nearest = Some((reach, foe));
        }
    }
    (count >= min_matches).then(|| nearest.map(|(_, target)| target))
}

impl RotwkRules {
    pub(super) fn step_auto_casts(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let now = tick.frame();
        let mut casters: Vec<Entity> = tick
            .state()
            .store(wiring.auto_ability_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        casters.sort_unstable_by_key(|entity| entity.index());
        casters.dedup();
        for entity in casters {
            if tick.state().contains(ids.doomed, entity)
                || tick
                    .state()
                    .get(ids.health, entity)
                    .is_some_and(|health| health.0.is_dead())
            {
                continue;
            }
            let ordered = tick.state().contains(ids.player_order, entity);
            let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
                continue;
            };
            let Some(kind) = tick.state().get(ids.kind, entity).copied() else {
                continue;
            };
            let Some(position) = tick
                .state()
                .get(ids.body, entity)
                .map(|body| body.0.position)
            else {
                continue;
            };
            let due: Vec<(usize, mint_sage_modules::AutoAbilityBehavior)> = {
                let Some(column) = tick.state_mut().get_mut(wiring.auto_ability_state, entity)
                else {
                    continue;
                };
                column
                    .0
                    .iter_mut()
                    .enumerate()
                    .filter_map(|(index, slot)| {
                        slot.state.note_order(now, ordered);
                        (slot.state.ready_to_scan(now) && slot.state.idle_long_enough(now))
                            .then(|| (index, slot.state.clone()))
                    })
                    .collect()
            };
            if due.is_empty() {
                continue;
            }
            let set = self
                .command_set_override(tick.state(), entity)
                .map(str::to_owned)
                .or_else(|| {
                    self.kinds
                        .get(kind)
                        .and_then(|entry| entry.record.command_set.clone())
                });
            let auto_buttons: Vec<AutoButton> = self
                .kinds
                .get(kind)
                .map(|entry| entry.auto_power_under(set.as_deref()).to_vec())
                .unwrap_or_default();
            let mut cast_this_tick: Vec<String> = Vec::new();
            for (index, module) in due {
                let next = match self.run_auto_scan(
                    tick,
                    ids,
                    entity,
                    owner,
                    position,
                    &module,
                    &auto_buttons,
                    living,
                    grid,
                    &mut cast_this_tick,
                )? {
                    Some(delay) => now.checked_add(delay).unwrap_or(Frame::FOREVER),
                    None => now.checked_add(SCAN_INTERVAL).unwrap_or(Frame::FOREVER),
                };
                if let Some(column) = tick.state_mut().get_mut(wiring.auto_ability_state, entity) {
                    if let Some(slot) = column.0.get_mut(index) {
                        slot.state.defer_scan(next);
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn autocast_enabled(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        power_name: &str,
    ) -> bool {
        self.autocast_holder(tick.state(), ids, entity)
            .is_some_and(|held| held.eq_ignore_ascii_case(power_name))
    }

    pub(crate) fn autocast_holder(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<String> {
        match world.get(ids.auto_cast, entity) {
            Some(slot) => slot.0.clone(),
            None => self.autocast_default_holder(world, ids, entity),
        }
    }

    fn autocast_default_holder(&self, world: &World, ids: Ids, entity: Entity) -> Option<String> {
        let wiring = self.modules.as_ref()?;
        let column = world.get(wiring.auto_ability_state, entity)?;
        let kind = world.get(ids.kind, entity).copied()?;
        let entry = self.kinds.get(kind)?;
        let set = self.command_set_override(world, entity);
        let auto_buttons = entry.auto_power_under(set);
        column.0.iter().find_map(|slot| {
            if !slot.state.starts_active() {
                return None;
            }
            match slot.state.power() {
                Some(named) => auto_buttons
                    .iter()
                    .any(|button| button.power.eq_ignore_ascii_case(named))
                    .then(|| named.to_owned()),
                None => auto_buttons.first().map(|button| button.power.clone()),
            }
        })
    }

    pub(super) fn toggle_auto_cast(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        power_name: &str,
    ) -> Result<()> {
        let was_holder = self.autocast_enabled(tick, ids, entity, power_name);
        let slot = AutoCastSlot(if was_holder {
            None
        } else {
            Some(power_name.to_ascii_lowercase())
        });
        tick.state_mut().insert(ids.auto_cast, entity, slot)?;
        diag!(
            "power: {power_name} auto-cast at entity {} set to {}",
            entity.index(),
            !was_holder
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn run_auto_scan(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        owner: PlayerId,
        position: Vec2,
        module: &mint_sage_modules::AutoAbilityBehavior,
        auto_buttons: &[AutoButton],
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
        cast_this_tick: &mut Vec<String>,
    ) -> Result<Option<LogicFrames>> {
        let named = module.power();
        let candidates: Vec<&AutoButton> = auto_buttons
            .iter()
            .filter(|button| named.is_none_or(|name| button.power.eq_ignore_ascii_case(name)))
            .filter(|button| {
                !cast_this_tick
                    .iter()
                    .any(|cast| cast.eq_ignore_ascii_case(&button.power))
            })
            .filter(|button| self.autocast_enabled(tick, ids, entity, &button.power))
            .collect();
        if candidates.is_empty() {
            return Ok(None);
        }
        let default_query = [(1u32, ObjectFilter::default())];
        let query: &[mint_sage_modules::Query] = if module.query().is_empty() {
            &default_query
        } else {
            module.query()
        };
        for button in candidates {
            if self.disallowed_by_model_condition(tick, entity, &button.disallowed)
                || !self.model_conditions_gate(
                    tick,
                    entity,
                    &button.enable_on_model_condition,
                    &button.disable_on_model_condition,
                )
            {
                continue;
            }
            let Some(target) = self.scan_for_target(
                tick, ids, entity, owner, position, module, button, query, living, grid,
            ) else {
                continue;
            };
            let (target, at) = match button.target {
                TargetKind::None | TargetKind::Confirm => (None, None),
                TargetKind::Object => (Some(target), None),
                TargetKind::Position => {
                    let Some(at) = tick
                        .state()
                        .get(ids.body, target)
                        .map(|body| body.0.position)
                    else {
                        continue;
                    };
                    (None, Some((at.x, at.y)))
                }
            };
            if self.fire_power_from(
                tick,
                ids,
                owner,
                entity,
                &button.power,
                button.target,
                &button.relation,
                target,
                at,
                true,
            )? {
                diag!(
                    "power: {} at entity {} auto-cast",
                    button.power,
                    entity.index()
                );
                cast_this_tick.push(button.power.clone());
                let delay = button
                    .auto_delay
                    .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
                    .map(LogicFrames::from_seconds)
                    .unwrap_or(AUTO_CAST_DEFAULT_DELAY);
                return Ok(Some(delay));
            }
        }
        Ok(None)
    }

    fn model_conditions_gate(
        &self,
        tick: &Tick<'_>,
        entity: Entity,
        enable: &[String],
        disable: &[String],
    ) -> bool {
        if enable.is_empty() && disable.is_empty() {
            return true;
        }
        let Some(wiring) = self.modules.as_ref() else {
            return enable.is_empty();
        };
        let Some(state) = tick.state().get(wiring.state, entity) else {
            return enable.is_empty();
        };
        let held = state.model_condition_names();
        enable
            .iter()
            .all(|flag| held.iter().any(|name| name.eq_ignore_ascii_case(flag)))
            && !disable
                .iter()
                .any(|flag| held.iter().any(|name| name.eq_ignore_ascii_case(flag)))
    }

    fn disallowed_by_model_condition(
        &self,
        tick: &Tick<'_>,
        entity: Entity,
        disallowed: &[String],
    ) -> bool {
        if disallowed.is_empty() {
            return false;
        }
        let Some(wiring) = self.modules.as_ref() else {
            return false;
        };
        let Some(state) = tick.state().get(wiring.state, entity) else {
            return false;
        };
        let held = state.model_condition_names();
        disallowed
            .iter()
            .any(|flag| held.iter().any(|name| name.eq_ignore_ascii_case(flag)))
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_for_target(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        owner: PlayerId,
        position: Vec2,
        module: &mint_sage_modules::AutoAbilityBehavior,
        button: &AutoButton,
        query: &[mint_sage_modules::Query],
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Option<Entity> {
        let max_range = if module.max_scan_range() < mint_sage_modules::UNBOUNDED_RANGE {
            module.max_scan_range()
        } else {
            button
                .preset_range
                .unwrap_or(mint_sage_modules::UNBOUNDED_RANGE)
        };
        let min_range = module.min_scan_range();
        let max_reach = max_range * max_range;
        let min_reach = min_range * min_range;
        for (min_matches, filter) in query {
            let passing = |index: u32| -> Option<(f32, Entity)> {
                let at = usize::try_from(index).ok()?;
                let (foe, other_owner) = living.get(at)?;
                if foe.entity == entity && !module.allow_self() {
                    return None;
                }
                if foe.not_autoacquirable {
                    return None;
                }
                let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
                let reach = dx * dx + dy * dy;
                if reach > max_reach || reach < min_reach {
                    return None;
                }
                let kind = tick.state().get(ids.kind, foe.entity).copied()?;
                let entry = self.kinds.get(kind)?;
                if !filter.passes(&entry.record.kind_of, &entry.record.name) {
                    return None;
                }
                let (same, allied, hostile) = super::spell_effects::standing(
                    self,
                    owner,
                    (foe.entity != entity).then_some(*other_owner),
                );
                if !filter.passes_relation(same, allied, hostile)
                    || !button.relation.passes_relation(same, allied, hostile)
                {
                    return None;
                }
                if self.disallowed_by_status(tick, foe.entity, module.forbidden_status()) {
                    return None;
                }
                if button.need_damaged_target {
                    let health = tick.state().get(ids.health, foe.entity)?;
                    if health.0.max() <= 0.0
                        || health.0.current() / health.0.max() > NEED_DAMAGED_TARGET_RATIO
                    {
                        return None;
                    }
                }
                Some((reach, foe.entity))
            };
            if let Some(answer) = nearest_by_expanding_scan(
                position,
                grid,
                min_range,
                max_range,
                *min_matches,
                passing,
            ) {
                return answer;
            }
        }
        None
    }

    fn disallowed_by_status(
        &self,
        tick: &Tick<'_>,
        entity: Entity,
        forbidden: &mint_sage_modules::effects::Names,
    ) -> bool {
        if forbidden.is_empty() {
            return false;
        }
        let Some(wiring) = self.modules.as_ref() else {
            return false;
        };
        let Some(state) = tick.state().get(wiring.state, entity) else {
            return false;
        };
        forbidden.iter().any(|flag| {
            state
                .status_bits
                .iter()
                .any(|held| held.eq_ignore_ascii_case(flag))
        })
    }
}
