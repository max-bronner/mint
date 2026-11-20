use mint_movement::Vec2;
use mint_sage_modules::{ActiveButton, AutoAbilityBehavior};
use mint_sim::{Entity, Frame, LogicFrames, PlayerId, Result, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{grid_cell, grid_radius_cells, Foe, TargetGrid};
use crate::defeat::ObjectFilter;
use crate::diag::diag;
use crate::templates::{AutoButton, MountGate, TargetKind};

const SCAN_INTERVAL: LogicFrames = LogicFrames::from_frames(36);

const AUTO_CAST_DEFAULT_DELAY: LogicFrames = LogicFrames::from_frames(10 * mint_sim::SIMULATION_HZ);

const NEED_DAMAGED_TARGET_RATIO: f32 = 0.8;

const SCAN_START_CELLS: i32 = 1;

const STUN_CONDITIONS: [&str; 3] = ["STUNNED_FLAILING", "STUNNED", "STUNNED_STANDING_UP"];

pub(super) fn nearest_by_expanding_scan(
    position: Vec2,
    grid: &TargetGrid,
    min_range: f32,
    max_range: f32,
    min_matches: u32,
    passing: impl Fn(u32) -> Option<(f32, Entity)>,
) -> Option<Option<Entity>> {
    expanding_scan(position, grid, min_range, max_range, min_matches, |index| {
        passing(index).map(|(reach, foe)| (reach, foe, true))
    })
}

pub(super) fn expanding_scan(
    position: Vec2,
    grid: &TargetGrid,
    min_range: f32,
    max_range: f32,
    min_matches: u32,
    passing: impl Fn(u32) -> Option<(f32, Entity, bool)>,
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
            let Some((reach, foe, pickable)) = passing(*index) else {
                continue;
            };
            if reach > accept {
                continue;
            }
            count = count.saturating_add(1);
            let better = pickable
                && nearest.is_none_or(|(best_reach, best_entity)| {
                    reach < best_reach || (reach == best_reach && foe.index() < best_entity.index())
                });
            if better {
                nearest = Some((reach, foe));
            }
        }
        if last {
            return (count >= min_matches).then(|| nearest.map(|(_, target)| target));
        }
        if min_matches > 0 && count >= min_matches && nearest.is_some() {
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
            let due: Vec<(usize, AutoAbilityBehavior)> = tick
                .state()
                .get(wiring.auto_ability_state, entity)
                .map(|column| {
                    column
                        .0
                        .iter()
                        .enumerate()
                        .filter(|(_, slot)| slot.state.ready_to_scan(now))
                        .map(|(index, slot)| (index, slot.state.clone()))
                        .collect()
                })
                .unwrap_or_default();
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
                let sleep = self.run_auto_scan(
                    tick,
                    ids,
                    ScanCaster {
                        entity,
                        owner,
                        position,
                        ordered,
                    },
                    index,
                    &module,
                    &auto_buttons,
                    living,
                    grid,
                    &mut cast_this_tick,
                )?;
                let next = match sleep {
                    Sleep::For(delay) => now.checked_add(delay).unwrap_or(Frame::FOREVER),
                    Sleep::Forever => Frame::FOREVER,
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

    pub(crate) fn autocast_on(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        power_name: &str,
    ) -> bool {
        self.serving_module(world, ids, entity, power_name)
            .is_some_and(|(_, active)| {
                matches!(active, ActiveButton::On(held) if held.eq_ignore_ascii_case(power_name))
            })
    }

    pub(super) fn autocast_enabled(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        power_name: &str,
    ) -> bool {
        self.autocast_on(tick.state(), ids, entity, power_name)
    }

    fn serving_module(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        power_name: &str,
    ) -> Option<(usize, ActiveButton)> {
        let wiring = self.modules.as_ref()?;
        let column = world.get(wiring.auto_ability_state, entity)?;
        let kind = world.get(ids.kind, entity).copied()?;
        let entry = self.kinds.get(kind)?;
        let buttons = entry.auto_power_under(self.command_set_override(world, entity));
        if !buttons
            .iter()
            .any(|button| button.power.eq_ignore_ascii_case(power_name))
        {
            return None;
        }
        let (index, slot) = column.0.iter().enumerate().find(|(_, slot)| {
            slot.state
                .power()
                .is_none_or(|named| named.eq_ignore_ascii_case(power_name))
        })?;
        let active = match &slot.state.active {
            ActiveButton::Unresolved => default_active(&slot.state, buttons),
            resolved => resolved.clone(),
        };
        Some((index, active))
    }

    pub(super) fn toggle_auto_cast(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        power_name: &str,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let Some((index, active)) = self.serving_module(tick.state(), ids, entity, power_name)
        else {
            return Ok(());
        };
        let on = matches!(&active, ActiveButton::On(held) if held.eq_ignore_ascii_case(power_name));
        let wake = tick
            .frame()
            .checked_add(LogicFrames::from_frames(1))
            .unwrap_or(Frame::FOREVER);
        if let Some(slot) = tick
            .state_mut()
            .get_mut(wiring.auto_ability_state, entity)
            .and_then(|column| column.0.get_mut(index))
        {
            if on {
                slot.state.active = ActiveButton::Off;
            } else {
                slot.state.active = ActiveButton::On(power_name.to_ascii_lowercase());
                slot.state.defer_scan(wake);
            }
        }
        diag!(
            "power: {power_name} auto-cast at entity {} set to {}",
            entity.index(),
            !on
        );
        Ok(())
    }

    fn set_module_switch(
        &self,
        tick: &mut Tick<'_>,
        entity: Entity,
        index: usize,
        active: ActiveButton,
    ) {
        let Some(wiring) = self.modules.as_ref() else {
            return;
        };
        if let Some(slot) = tick
            .state_mut()
            .get_mut(wiring.auto_ability_state, entity)
            .and_then(|column| column.0.get_mut(index))
        {
            slot.state.active = active;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_auto_scan(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        caster: ScanCaster,
        index: usize,
        module: &AutoAbilityBehavior,
        auto_buttons: &[AutoButton],
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
        cast_this_tick: &mut Vec<String>,
    ) -> Result<Sleep> {
        let ScanCaster {
            entity,
            owner,
            position,
            ordered,
        } = caster;
        let now = tick.frame();
        let active = match &module.active {
            ActiveButton::Unresolved => {
                let resolved = default_active(module, auto_buttons);
                self.set_module_switch(tick, entity, index, resolved.clone());
                resolved
            }
            held => held.clone(),
        };
        let ActiveButton::On(power) = active else {
            return Ok(Sleep::Forever);
        };
        let Some(button) = auto_buttons
            .iter()
            .find(|button| button.power.eq_ignore_ascii_case(&power))
        else {
            return Ok(Sleep::For(SCAN_INTERVAL));
        };
        if !self.button_gates_open(tick.state(), entity, button) {
            self.set_module_switch(tick, entity, index, ActiveButton::Off);
            return Ok(Sleep::Forever);
        }
        if !module.idle_time().is_zero()
            && (ordered || Self::ai_busy(tick.state(), ids, entity, now))
        {
            return Ok(Sleep::For(module.idle_time()));
        }
        if self.auto_caster_blocked(tick, ids, entity, module, button) {
            return Ok(Sleep::For(SCAN_INTERVAL));
        }
        if cast_this_tick
            .iter()
            .any(|cast| cast.eq_ignore_ascii_case(&button.power))
        {
            return Ok(Sleep::For(SCAN_INTERVAL));
        }
        let (target, at) = if button.trigger_when_ready {
            (None, None)
        } else {
            let radius = self.scan_radius(tick, ids, entity, module, button);
            let centre = if module.base_max_range_from_start_pos() {
                let kept = module.start_position.unwrap_or((position.x, position.y));
                if module.start_position.is_none() {
                    self.keep_start_position(tick, entity, index, kept);
                }
                Vec2::new(kept.0, kept.1)
            } else {
                position
            };
            let found = if module.query().is_empty() {
                self.closest_enemy(tick, ids, entity, owner, centre, radius, living, grid)
            } else {
                self.scan_for_target(
                    tick, ids, entity, owner, centre, radius, module, button, living, grid,
                )
            };
            let Some(found) = found else {
                return Ok(Sleep::For(SCAN_INTERVAL));
            };
            match button.target {
                TargetKind::None | TargetKind::Confirm => (None, None),
                TargetKind::Object => (Some(found), None),
                TargetKind::Position => {
                    let Some(at) = tick
                        .state()
                        .get(ids.body, found)
                        .map(|body| body.0.position)
                    else {
                        return Ok(Sleep::For(SCAN_INTERVAL));
                    };
                    (None, Some((at.x, at.y)))
                }
            }
        };
        let press_kind = if button.trigger_when_ready {
            TargetKind::None
        } else {
            button.target
        };
        if self.fire_power_from(
            tick,
            ids,
            owner,
            entity,
            &button.power,
            press_kind,
            &ObjectFilter::default(),
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
            return Ok(Sleep::For(delay));
        }
        Ok(Sleep::For(SCAN_INTERVAL))
    }

    fn keep_start_position(
        &self,
        tick: &mut Tick<'_>,
        entity: Entity,
        index: usize,
        at: (f32, f32),
    ) {
        let Some(wiring) = self.modules.as_ref() else {
            return;
        };
        if let Some(slot) = tick
            .state_mut()
            .get_mut(wiring.auto_ability_state, entity)
            .and_then(|column| column.0.get_mut(index))
        {
            slot.state.start_position = Some(at);
        }
    }

    fn button_gates_open(&self, world: &World, entity: Entity, button: &AutoButton) -> bool {
        let mounted = self.holds_condition_in(world, entity, "MOUNTED");
        let mount_ok = match button.mount_gate {
            MountGate::Either => true,
            MountGate::MountedOnly => mounted,
            MountGate::UnmountedOnly => !mounted,
        };
        mount_ok
            && self.model_condition_gate(
                world,
                entity,
                &button.enable_on_model_condition,
                &button.disable_on_model_condition,
            )
    }

    fn auto_caster_blocked(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        module: &AutoAbilityBehavior,
        button: &AutoButton,
    ) -> bool {
        let world = tick.state();
        module
            .forbidden_status()
            .iter()
            .any(|status| self.carries_object_status(world, entity, status))
            || STUN_CONDITIONS
                .iter()
                .any(|name| self.holds_condition_in(world, entity, name))
            || self.is_hidden(world, ids, entity)
            || self.disallowed_by_model_condition(tick, entity, &button.disallowed)
            || world
                .get(ids.ignore_ai_command, entity)
                .is_some_and(|held| tick.frame() < held.0)
            || self.carries_object_status(world, entity, "IGNORE_AI_COMMAND")
            || world.contains(ids.no_auto_acquire, entity)
    }

    fn scan_radius(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        module: &AutoAbilityBehavior,
        button: &AutoButton,
    ) -> f32 {
        if module.max_scan_range() < mint_sage_modules::UNBOUNDED_RANGE {
            return module.max_scan_range();
        }
        if let Some(preset) = button.preset_range {
            return preset;
        }
        let world = tick.state();
        let Some(entry) = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
        else {
            return 0.0;
        };
        let Some(base) = entry
            .record
            .vision_range
            .filter(|v| v.is_finite() && *v > 0.0)
        else {
            return 0.0;
        };
        let bonus = world
            .get(ids.bonuses, entity)
            .map_or(0.0, |bonuses| bonuses.0.vision);
        let rise = entry
            .record
            .vision_bonus
            .as_ref()
            .zip(world.get(ids.body, entity))
            .map_or(0.0, |(bonus, body)| {
                let ground = |x, y| self.passability.heights.height_at(x, y);
                let at = body.0.position;
                crate::combat::ground_rise(bonus, &ground, at.x, at.y)
                    .map_or(0.0, |rise| crate::combat::height_bonus(bonus, rise))
            });
        crate::combat::vision_reach(base * (1.0 + bonus).max(0.0), rise)
    }

    #[allow(clippy::too_many_arguments)]
    fn closest_enemy(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Option<Entity> {
        let world = tick.state();
        if !world.contains(ids.armed, entity) || Self::holds_fire(world, ids, entity) {
            return None;
        }
        let reach = radius * radius;
        let passing = |index: u32| -> Option<(f32, Entity)> {
            let (foe, other_owner) = living.get(usize::try_from(index).ok()?)?;
            if foe.entity == entity
                || foe.enclosed
                || foe.unattackable
                || foe.not_autoacquirable
                || world.contains(ids.horde, foe.entity)
            {
                return None;
            }
            let (dx, dy) = (foe.position.x - centre.x, foe.position.y - centre.y);
            let away = dx * dx + dy * dy;
            if away > reach {
                return None;
            }
            let (_, _, hostile) = super::spell_effects::standing(self, owner, Some(*other_owner));
            hostile.then_some((away, foe.entity))
        };
        nearest_by_expanding_scan(centre, grid, 0.0, radius, 1, passing)?
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_for_target(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        owner: PlayerId,
        centre: Vec2,
        max_range: f32,
        module: &AutoAbilityBehavior,
        button: &AutoButton,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Option<Entity> {
        let min_range = module.min_scan_range();
        let max_reach = max_range * max_range;
        let min_reach = min_range * min_range;
        for (min_matches, filter) in module.query() {
            let passing = |index: u32| -> Option<(f32, Entity, bool)> {
                let at = usize::try_from(index).ok()?;
                let (foe, other_owner) = living.get(at)?;
                if foe.not_autoacquirable
                    || foe.enclosed
                    || self.carries_object_status(tick.state(), foe.entity, "INSIDE_GARRISON")
                {
                    return None;
                }
                let (dx, dy) = (foe.position.x - centre.x, foe.position.y - centre.y);
                let reach = dx * dx + dy * dy;
                if reach > max_reach {
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
                if !filter.passes_relation(same, allied, hostile) {
                    return None;
                }
                let damaged_ok = !button.need_damaged_target
                    || tick
                        .state()
                        .get(ids.health, foe.entity)
                        .is_some_and(|health| {
                            health.0.max() > 0.0
                                && health.0.current() / health.0.max() <= NEED_DAMAGED_TARGET_RATIO
                        });
                let pickable = (module.allow_self() || foe.entity != entity)
                    && damaged_ok
                    && reach >= min_reach;
                Some((reach, foe.entity, pickable))
            };
            if let Some(Some(target)) =
                expanding_scan(centre, grid, min_range, max_range, *min_matches, passing)
            {
                return Some(target);
            }
        }
        None
    }

    pub(crate) fn model_condition_gate(
        &self,
        world: &World,
        entity: Entity,
        enable: &[String],
        disable: &[String],
    ) -> bool {
        if enable.is_empty() && disable.is_empty() {
            return true;
        }
        let held = self
            .modules
            .as_ref()
            .and_then(|wiring| world.get(wiring.state, entity))
            .map(mint_sage_modules::UpgradeState::model_condition_names)
            .unwrap_or_default();
        let holds_any = |flags: &[String]| {
            flags
                .iter()
                .any(|flag| held.iter().any(|name| name.eq_ignore_ascii_case(flag)))
        };
        !holds_any(disable) && (enable.is_empty() || holds_any(enable))
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
}

#[derive(Clone, Copy)]
struct ScanCaster {
    entity: Entity,
    owner: PlayerId,
    position: Vec2,
    ordered: bool,
}

#[derive(Clone, Copy)]
enum Sleep {
    For(LogicFrames),
    Forever,
}

fn default_active(module: &AutoAbilityBehavior, buttons: &[AutoButton]) -> ActiveButton {
    if !module.starts_active() {
        return ActiveButton::Off;
    }
    let button = match module.power() {
        Some(named) => buttons
            .iter()
            .find(|button| button.power.eq_ignore_ascii_case(named)),
        None => buttons.first(),
    };
    button.map_or(ActiveButton::Off, |button| {
        ActiveButton::On(button.power.to_ascii_lowercase())
    })
}
