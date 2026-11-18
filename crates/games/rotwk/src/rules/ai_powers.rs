use mint_movement::Vec2;
use mint_sage_ini::Stance;
use mint_sim::{
    ai_power::{AiPowerCandidate, AiPowerContext, AiPowerTarget},
    Component, Entity, Frame, PlayerId, Result, StateHasher, Tick, World,
};

use super::{Ids, RotwkRules};
use crate::alliance::Stance as AllianceStance;
use crate::combat::{grid_cell, grid_radius_cells, Foe, TargetGrid};
use crate::diag::diag;
use crate::templates::{target_kind, target_relation, TargetKind};

const TICKS_PER_FRAME: u32 = mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;

const REFIRE_FRAMES: u32 = 25;

const ENEMY_TYPE_KILLER_RANGED_MIN_DISTANCE: f32 = 100.0;

const RANGED_AOE_FAR_ENOUGH_FRACTION: f32 = 0.8;

const TARGETAOE_SUMMON_MIN_COUNT: u32 = 2;

const GIVEXP_AOE_MIN_COUNT: u32 = 1;

const ENEMY_TYPE_KILLER_KINDS: [&str; 4] = ["HERO", "CREATE_A_HERO", "BIG_MONSTER", "MONSTER"];

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AiPowerRefire(pub(crate) Vec<Frame>);

impl Component for AiPowerRefire {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(u32::try_from(self.0.len()).unwrap_or(u32::MAX));
        for frame in &self.0 {
            frame.hash_into(hasher);
        }
    }
}

fn ai_power_due(now: Frame, entity: Entity) -> bool {
    let phase = now.get().wrapping_add(u64::from(entity.index()));
    phase % u64::from(TICKS_PER_FRAME) == 0
}

enum AiPowerAction {
    Stance(Stance),
    Power(String),
}

impl RotwkRules {
    pub(super) fn step_ai_special_powers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let mut casters: Vec<Entity> = tick
            .state()
            .store(wiring.ai_power_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        casters.sort_unstable_by_key(|entity| entity.index());
        casters.dedup();
        let now = tick.frame();
        for entity in casters {
            if !ai_power_due(now, entity) {
                continue;
            }
            if !tick.state().is_live(entity) || tick.state().contains(ids.doomed, entity) {
                continue;
            }
            let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
                continue;
            };
            if !self.computer_player(owner) {
                continue;
            }
            let health_ratio = match tick.state().get(ids.health, entity) {
                Some(health) if health.0.is_dead() => continue,
                Some(health) => health.0.current() / health.0.max().max(f32::EPSILON),
                None => match tick.state().get(ids.horde, entity) {
                    Some(horde) if horde.members.is_empty() => continue,
                    Some(horde) => Self::horde_health_ratio(horde.members.len(), horde.slots.len()),
                    None => continue,
                },
            };
            let Some(position) = tick
                .state()
                .get(ids.body, entity)
                .map(|body| body.0.position)
            else {
                continue;
            };
            let in_combat = self.ai_power_in_combat(tick.state(), ids, entity);
            let instances: Vec<mint_sage_modules::AISpecialPowerUpdate> = {
                let Some(column) = tick.state().get(wiring.ai_power_state, entity) else {
                    continue;
                };
                column.0.iter().map(|slot| slot.state.clone()).collect()
            };
            for (index, module) in instances.iter().enumerate() {
                self.try_ai_special_power(
                    tick,
                    ids,
                    entity,
                    index,
                    owner,
                    health_ratio,
                    in_combat,
                    position,
                    module,
                    living,
                    grid,
                )?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn try_ai_special_power(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        index: usize,
        owner: PlayerId,
        health_ratio: f32,
        in_combat: bool,
        position: Vec2,
        module: &mint_sage_modules::AISpecialPowerUpdate,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let Some(kind) = mint_sage_modules::ai_power_registry()
            .get(module.special_power_ai_type())
            .cloned()
        else {
            return Ok(());
        };
        let Some(button) = self.ai_power_buttons.get(module.command_button_name()) else {
            return Ok(());
        };
        let is_set_stance = button
            .command
            .as_deref()
            .is_some_and(|command| command.eq_ignore_ascii_case("SET_STANCE"));
        let is_special_power = button
            .command
            .as_deref()
            .is_some_and(|command| command.eq_ignore_ascii_case("SPECIAL_POWER"));
        let (current_mode, action) = if is_set_stance {
            let Some(stance) = button
                .stances
                .first()
                .and_then(|name| Stance::from_name(name))
            else {
                return Ok(());
            };
            (
                self.stance_of(tick.state(), ids, entity).map(Stance::index),
                AiPowerAction::Stance(stance),
            )
        } else if is_special_power {
            let Some(power) = button.special_power.clone() else {
                return Ok(());
            };
            (None, AiPowerAction::Power(power))
        } else {
            return Ok(());
        };
        let (nearby_enemies, nearby_allies) = self.count_nearby_standing(
            tick.state(),
            entity,
            position,
            mint_sage_modules::SELF_BUFF_SCAN_RADIUS,
            living,
            grid,
        );
        let type_name = module.special_power_ai_type();
        let candidates = self.ai_power_candidates(
            tick.state(),
            ids,
            entity,
            position,
            type_name,
            module,
            living,
            grid,
        );
        let (victim_distance, weapon_is_melee) =
            self.ai_power_weapon_state(tick.state(), ids, entity, position);
        if tick.rng().next_f32() <= 0.5 {
            return Ok(());
        }
        let rng_unit = if type_name == "AI_SPECIAL_POWER_SELFAOEHEALHEROS" {
            tick.rng().next_f32()
        } else {
            0.0
        };
        let ctx = AiPowerContext {
            health_ratio,
            current_mode,
            nearby_enemies,
            nearby_allies,
            in_combat,
            rng_unit,
            victim_distance,
            weapon_is_melee,
        };
        if !kind.should_activate(ctx, &candidates) {
            return Ok(());
        }
        if !self.ai_power_rate_gate(tick, owner) {
            return Ok(());
        }
        if !self.ai_power_refire_ready(tick, ids, entity, index) {
            return Ok(());
        }
        match action {
            AiPowerAction::Stance(stance) => {
                self.apply_stance_change(tick, ids, entity, stance)?;
                diag!(
                    "power: AI entity {} sets stance {}",
                    entity.index(),
                    stance.as_str()
                );
            }
            AiPowerAction::Power(power) => {
                let kind_of_target = target_kind(&button.options);
                let (target_entity, target_pos) =
                    match (kind_of_target, kind.choose_target(&candidates)) {
                        (TargetKind::None | TargetKind::Confirm, None) => (None, None),
                        (TargetKind::Object, Some(AiPowerTarget::Object(found))) => {
                            (Some(found), None)
                        }
                        (TargetKind::Position, Some(AiPowerTarget::Position(x, y))) => {
                            (None, Some((x, y)))
                        }
                        _ => return Ok(()),
                    };
                let relation = target_relation(&button.options);
                let fired = self.fire_power_from(
                    tick,
                    ids,
                    owner,
                    entity,
                    &power,
                    kind_of_target,
                    &relation,
                    target_entity,
                    target_pos,
                    true,
                )?;
                if fired {
                    diag!("power: AI entity {} fires {power}", entity.index());
                }
            }
        }
        Ok(())
    }

    fn ai_power_rate_gate(&self, tick: &mut Tick<'_>, owner: PlayerId) -> bool {
        const DEFAULT_ROW: usize = 2;
        let row = self
            .ai_power_difficulty
            .iter()
            .find(|(player, _)| *player == owner)
            .map_or(DEFAULT_ROW, |(_, difficulty)| {
                usize::from(difficulty.tuning_row())
            });
        let (a, b) = self
            .ai_power_activation_tuning
            .get(row)
            .copied()
            .unwrap_or((1, 1));
        let Ok(b32) = u32::try_from(b.max(1)) else {
            return true;
        };
        Self::rate_gate_passes(a, b, tick.rng().below(b32))
    }

    fn rate_gate_passes(a: i64, b: i64, draw: u32) -> bool {
        if b <= 0 || a >= b {
            return true;
        }
        let Ok(a32) = u32::try_from(a.max(0)) else {
            return false;
        };
        draw < a32
    }

    fn ai_power_refire_ready(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        index: usize,
    ) -> bool {
        let now = tick.frame();
        let mut refire = tick
            .state()
            .get(ids.ai_power_refire, entity)
            .cloned()
            .unwrap_or_default();
        if refire.0.len() <= index {
            refire.0.resize(index.saturating_add(1), now);
        }
        let Some(last_reset) = refire.0.get(index).copied() else {
            return false;
        };
        if !Self::refire_due(now, last_reset) {
            let _ = tick.state_mut().insert(ids.ai_power_refire, entity, refire);
            return false;
        }
        if let Some(slot) = refire.0.get_mut(index) {
            *slot = now;
        }
        tick.state_mut()
            .insert(ids.ai_power_refire, entity, refire)
            .is_ok()
    }

    fn horde_health_ratio(members: usize, slots: usize) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let ratio = members as f32 / slots.max(1) as f32;
        ratio
    }

    fn refire_due(now: Frame, last_reset: Frame) -> bool {
        let threshold = u64::from(REFIRE_FRAMES) * u64::from(TICKS_PER_FRAME);
        now.get().saturating_sub(last_reset.get()) >= threshold
    }

    fn object_stance(&self, world: &World, me: Entity, that: Entity) -> AllianceStance {
        match self.object_relation(world, me, that) {
            super::map_scripts::Relation::Enemies => AllianceStance::Hostile,
            super::map_scripts::Relation::Allies => AllianceStance::Allied,
            super::map_scripts::Relation::Neutral => AllianceStance::Neutral,
        }
    }

    fn ai_power_in_combat(&self, world: &mint_sim::World, ids: Ids, entity: Entity) -> bool {
        if world
            .get(ids.attack_order, entity)
            .is_some_and(|order| !world.contains(ids.doomed, order.target))
        {
            return true;
        }
        if world
            .get(ids.armed, entity)
            .is_some_and(|armed| armed.target.is_some())
        {
            return true;
        }
        world
            .get(ids.horde_target, entity)
            .is_some_and(|target| target.target.is_some())
    }

    fn count_nearby_standing(
        &self,
        world: &World,
        caster: Entity,
        position: Vec2,
        radius: f32,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> (u32, u32) {
        let mut near: Vec<u32> = Vec::new();
        grid.query_into(
            grid_cell(position.x),
            grid_cell(position.y),
            grid_radius_cells(radius),
            &mut near,
        );
        let reach = radius * radius;
        let mut enemies: u32 = 0;
        let mut allies: u32 = 0;
        for index in &near {
            let Some(at) = usize::try_from(*index).ok() else {
                continue;
            };
            let Some((foe, _)) = living.get(at) else {
                continue;
            };
            if foe.not_autoacquirable {
                continue;
            }
            let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
            if dx * dx + dy * dy > reach {
                continue;
            }
            match self.object_stance(world, caster, foe.entity) {
                AllianceStance::Hostile => enemies = enemies.saturating_add(1),
                AllianceStance::Allied => allies = allies.saturating_add(1),
                AllianceStance::Neutral => {}
            }
        }
        (enemies, allies)
    }

    fn entity_health_ratio(&self, world: &World, ids: Ids, entity: Entity) -> Option<f32> {
        if let Some(health) = world.get(ids.health, entity) {
            return Some(if health.0.is_dead() {
                0.0
            } else {
                health.0.current() / health.0.max().max(f32::EPSILON)
            });
        }
        let horde = world.get(ids.horde, entity)?;
        if horde.members.is_empty() {
            return Some(0.0);
        }
        Some(Self::horde_health_ratio(
            horde.members.len(),
            horde.slots.len(),
        ))
    }

    fn entity_has_any_kind(&self, world: &World, ids: Ids, entity: Entity, mask: &[&str]) -> bool {
        mask.is_empty()
            || world
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
                .is_some_and(|entry| mask.iter().any(|name| entry.record.has_kind(name)))
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_enemies(
        &self,
        world: &World,
        ids: Ids,
        caster: Entity,
        position: Vec2,
        radius: f32,
        min_distance: f32,
        kind_mask: &[&str],
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Vec<AiPowerCandidate> {
        let mut near: Vec<u32> = Vec::new();
        grid.query_into(
            grid_cell(position.x),
            grid_cell(position.y),
            grid_radius_cells(radius),
            &mut near,
        );
        let reach = radius * radius;
        let floor = min_distance * min_distance;
        let mut found: Vec<AiPowerCandidate> = Vec::new();
        for index in &near {
            let Some(at) = usize::try_from(*index).ok() else {
                continue;
            };
            let Some((foe, _)) = living.get(at) else {
                continue;
            };
            if foe.not_autoacquirable {
                continue;
            }
            if self.object_stance(world, caster, foe.entity) != AllianceStance::Hostile {
                continue;
            }
            if !self.entity_has_any_kind(world, ids, foe.entity, kind_mask) {
                continue;
            }
            let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
            let distance_sq = dx * dx + dy * dy;
            if distance_sq > reach || distance_sq < floor {
                continue;
            }
            let health_ratio = self
                .entity_health_ratio(world, ids, foe.entity)
                .unwrap_or(1.0);
            found.push(AiPowerCandidate {
                entity: foe.entity,
                distance: distance_sq.sqrt(),
                health_ratio,
                position: (foe.position.x, foe.position.y),
            });
        }
        found.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| a.entity.index().cmp(&b.entity.index()))
        });
        found
    }

    fn count_at(
        &self,
        world: &World,
        caster: Entity,
        position: Vec2,
        radius: f32,
        enemies: bool,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> u32 {
        let mut near: Vec<u32> = Vec::new();
        grid.query_into(
            grid_cell(position.x),
            grid_cell(position.y),
            grid_radius_cells(radius),
            &mut near,
        );
        let reach = radius * radius;
        let mut count: u32 = 0;
        for index in &near {
            let Some(at) = usize::try_from(*index).ok() else {
                continue;
            };
            let Some((foe, _)) = living.get(at) else {
                continue;
            };
            if foe.not_autoacquirable {
                continue;
            }
            let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
            if dx * dx + dy * dy > reach {
                continue;
            }
            let stance = self.object_stance(world, caster, foe.entity);
            let matches = if enemies {
                stance == AllianceStance::Hostile
            } else {
                stance == AllianceStance::Allied
            };
            if matches {
                count = count.saturating_add(1);
            }
        }
        count
    }

    #[allow(clippy::too_many_arguments)]
    fn probe_candidates(
        &self,
        world: &World,
        position: Vec2,
        probe_radius: f32,
        min_count: u32,
        enemies: bool,
        caster: Entity,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Vec<AiPowerCandidate> {
        const OFFSETS: [(f32, f32); 4] = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
        let mut best: Option<((f32, f32), u32)> = None;
        for (ox, oy) in OFFSETS {
            let probe = (
                position.x + ox * probe_radius,
                position.y + oy * probe_radius,
            );
            let count = self.count_at(
                world,
                caster,
                Vec2::new(probe.0, probe.1),
                probe_radius,
                enemies,
                living,
                grid,
            );
            if best.is_none_or(|(_, best_count)| count > best_count) {
                best = Some((probe, count));
            }
        }
        match best {
            Some((probe_position, count)) if count >= min_count => vec![AiPowerCandidate {
                entity: caster,
                distance: 0.0,
                health_ratio: 1.0,
                position: probe_position,
            }],
            _ => Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_allies_heroes(
        &self,
        world: &World,
        ids: Ids,
        caster: Entity,
        position: Vec2,
        radius: f32,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Vec<AiPowerCandidate> {
        let mut near: Vec<u32> = Vec::new();
        grid.query_into(
            grid_cell(position.x),
            grid_cell(position.y),
            grid_radius_cells(radius),
            &mut near,
        );
        let reach = radius * radius;
        let mut found: Vec<AiPowerCandidate> = Vec::new();
        for index in &near {
            let Some(at) = usize::try_from(*index).ok() else {
                continue;
            };
            let Some((foe, _)) = living.get(at) else {
                continue;
            };
            if foe.entity == caster || foe.not_autoacquirable {
                continue;
            }
            if self.object_stance(world, caster, foe.entity) != AllianceStance::Allied {
                continue;
            }
            if !self.entity_has_any_kind(world, ids, foe.entity, &["HERO"]) {
                continue;
            }
            let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
            let distance_sq = dx * dx + dy * dy;
            if distance_sq > reach {
                continue;
            }
            let health_ratio = self
                .entity_health_ratio(world, ids, foe.entity)
                .unwrap_or(1.0);
            found.push(AiPowerCandidate {
                entity: foe.entity,
                distance: distance_sq.sqrt(),
                health_ratio,
                position: (foe.position.x, foe.position.y),
            });
        }
        found.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| a.entity.index().cmp(&b.entity.index()))
        });
        found
    }

    fn ai_power_weapon_state(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        position: Vec2,
    ) -> (Option<f32>, bool) {
        let Some(armed) = world.get(ids.armed, entity) else {
            return (None, false);
        };
        let weapon_is_melee = self
            .combat
            .weapons
            .get(armed.weapon)
            .is_some_and(|template| template.melee);
        let victim = world
            .get(ids.attack_order, entity)
            .map(|order| order.target)
            .or(armed.target);
        let victim_distance = victim
            .and_then(|victim| world.get(ids.body, victim))
            .map(|body| {
                let (dx, dy) = (
                    body.0.position.x - position.x,
                    body.0.position.y - position.y,
                );
                (dx * dx + dy * dy).sqrt()
            });
        (victim_distance, weapon_is_melee)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn ai_power_candidates(
        &self,
        world: &World,
        ids: Ids,
        caster: Entity,
        position: Vec2,
        type_name: &str,
        module: &mint_sage_modules::AISpecialPowerUpdate,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Vec<AiPowerCandidate> {
        let radius = module
            .special_power_radius()
            .or_else(|| module.special_power_range())
            .unwrap_or(0.0);
        match type_name.to_ascii_uppercase().as_str() {
            "AI_SPECIAL_POWER_CHARGE" => {
                self.scan_enemies(world, ids, caster, position, radius, 0.0, &[], living, grid)
            }
            "AI_SPECIAL_POWER_ENEMY_TYPE_KILLER" => self.scan_enemies(
                world,
                ids,
                caster,
                position,
                radius,
                0.0,
                &ENEMY_TYPE_KILLER_KINDS,
                living,
                grid,
            ),
            "AI_SPECIAL_POWER_ENEMY_TYPE_KILLER_RANGED" => self.scan_enemies(
                world,
                ids,
                caster,
                position,
                radius,
                ENEMY_TYPE_KILLER_RANGED_MIN_DISTANCE,
                &ENEMY_TYPE_KILLER_KINDS,
                living,
                grid,
            ),
            "AI_SPECIAL_POWER_RANGED_AOE_ATTACK" => {
                let range = module.special_power_range().unwrap_or(0.0);
                let nearest =
                    self.scan_enemies(world, ids, caster, position, range, 0.0, &[], living, grid);
                match nearest.into_iter().next() {
                    Some(candidate)
                        if candidate.distance >= range * RANGED_AOE_FAR_ENOUGH_FRACTION =>
                    {
                        vec![candidate]
                    }
                    _ => Vec::new(),
                }
            }
            "AI_SPECIAL_POWER_TARGETAOE_SUMMON" => self.probe_candidates(
                world,
                position,
                radius,
                TARGETAOE_SUMMON_MIN_COUNT,
                true,
                caster,
                living,
                grid,
            ),
            "AI_SPECIAL_POWER_GIVEXP_AOE" => self.probe_candidates(
                world,
                position,
                radius,
                GIVEXP_AOE_MIN_COUNT,
                false,
                caster,
                living,
                grid,
            ),
            "AI_SPECIAL_POWER_SELFAOEHEALHEROS" => {
                self.scan_allies_heroes(world, ids, caster, position, radius, living, grid)
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::World;

    #[test]
    fn an_entity_is_due_once_per_retail_frame_not_every_tick() {
        assert_eq!(TICKS_PER_FRAME, 6);
        let mut world = World::default();
        let entity = world.spawn().unwrap();
        let due: Vec<u64> = (0..18)
            .filter(|&tick| ai_power_due(Frame::new(tick), entity))
            .collect();
        assert_eq!(due, vec![0, 6, 12]);
    }

    #[test]
    fn different_entities_stagger_to_different_ticks() {
        let mut world = World::default();
        let a = world.spawn().unwrap();
        let b = world.spawn().unwrap();
        let first_due = |entity: Entity| {
            (0..TICKS_PER_FRAME.into())
                .find(|&tick| ai_power_due(Frame::new(tick), entity))
                .unwrap()
        };
        assert_ne!(first_due(a), first_due(b));
    }

    #[test]
    fn the_rate_gate_passes_outright_once_a_reaches_b() {
        assert!(RotwkRules::rate_gate_passes(1, 1, 0));
        assert!(RotwkRules::rate_gate_passes(4, 1, 0));
        assert!(RotwkRules::rate_gate_passes(3, 3, u32::MAX));
    }

    #[test]
    fn the_rate_gate_draws_below_a_out_of_b() {
        assert!(RotwkRules::rate_gate_passes(1, 4, 0));
        assert!(!RotwkRules::rate_gate_passes(1, 4, 1));
        assert!(!RotwkRules::rate_gate_passes(0, 1, 0));
    }

    #[test]
    fn a_non_positive_denominator_always_passes() {
        assert!(RotwkRules::rate_gate_passes(0, 0, 0));
        assert!(RotwkRules::rate_gate_passes(0, -1, 0));
    }

    #[test]
    fn refire_is_due_at_exactly_150_ticks_not_before() {
        assert_eq!(u64::from(REFIRE_FRAMES) * u64::from(TICKS_PER_FRAME), 150);
        let last_reset = Frame::new(1_000);
        assert!(!RotwkRules::refire_due(Frame::new(1_149), last_reset));
        assert!(RotwkRules::refire_due(Frame::new(1_150), last_reset));
        assert!(RotwkRules::refire_due(Frame::new(1_151), last_reset));
    }

    #[test]
    fn a_horde_at_half_strength_reads_as_half_health() {
        assert_eq!(RotwkRules::horde_health_ratio(5, 10), 0.5);
        assert_eq!(RotwkRules::horde_health_ratio(10, 10), 1.0);
    }
}
