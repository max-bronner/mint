use mint_movement::Vec2;
use mint_sage_map::BasePhase;
use mint_sim::{Component, Entity, Frame, PlayerId, StateHasher};

use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AiTask {
    pub(crate) kind: Kind,
    pub(crate) position: Vec2,
    pub(crate) angle: f32,
    pub(crate) priority: i32,
    pub(crate) phase: BasePhase,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AiCandidate {
    pub(crate) name: String,
    pub(crate) tasks: Vec<AiTask>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AiSeed {
    pub(crate) player: mint_sim::PlayerId,
    pub(crate) candidates: Vec<AiCandidate>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ArmyPlan {
    pub(crate) player: mint_sim::PlayerId,
    pub(crate) members: Vec<ArmyMemberPlan>,
    pub(crate) must_use_command_point_percentage: [f32; 3],
    pub(crate) percent_to_save: [f32; 3],
    pub(crate) phase_duration_rush: f32,
    pub(crate) phase_duration_mid_game: f32,
    pub(crate) hero_build_order: Vec<Option<Kind>>,
    pub(crate) porter_kind: Option<Kind>,
    pub(crate) porter_target: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct AiArmyProgress {
    pub(crate) production_timer: u32,
    pub(crate) hero_index: u32,
}

impl Component for AiArmyProgress {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.production_timer);
        hasher.write_u32(self.hero_index);
    }
}

pub(crate) fn sorted_tasks(mut tasks: Vec<AiTask>) -> Vec<AiTask> {
    tasks.sort_by_key(|task| task.priority);
    tasks
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiTaskState {
    Pending,
    Ordered(Entity),
    Underway(Entity),
    Built(Entity),
    Destroyed(Frame),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiProgress {
    pub(crate) tasks: Vec<AiTaskState>,
    pub(crate) structure_timer: u32,
}

impl Component for AiProgress {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.tasks.len());
        for state in &self.tasks {
            match state {
                AiTaskState::Pending => hasher.write_u8(0),
                AiTaskState::Ordered(entity) => {
                    hasher.write_u8(1);
                    entity.hash_into(hasher);
                }
                AiTaskState::Underway(entity) => {
                    hasher.write_u8(4);
                    entity.hash_into(hasher);
                }
                AiTaskState::Built(entity) => {
                    hasher.write_u8(2);
                    entity.hash_into(hasher);
                }
                AiTaskState::Destroyed(frame) => {
                    hasher.write_u8(3);
                    frame.hash_into(hasher);
                }
            }
        }
        hasher.write_u32(self.structure_timer);
    }
}

pub(crate) fn allowed_phase(tasks: &[AiTask], states: &[AiTaskState]) -> BasePhase {
    let phase_built = |phase: BasePhase| {
        tasks
            .iter()
            .zip(states)
            .filter(|(task, _)| task.phase == phase)
            .all(|(_, state)| matches!(state, AiTaskState::Built(_)))
    };
    if !phase_built(BasePhase::One) {
        return BasePhase::One;
    }
    if !phase_built(BasePhase::Two) {
        return BasePhase::Two;
    }
    BasePhase::Three
}

pub(crate) fn retry_spiral(step: f32) -> [Vec2; 13] {
    [
        Vec2::new(0.0, 0.0),
        Vec2::new(step, 0.0),
        Vec2::new(0.0, step),
        Vec2::new(-step, 0.0),
        Vec2::new(0.0, -step),
        Vec2::new(step, step),
        Vec2::new(-step, step),
        Vec2::new(-step, -step),
        Vec2::new(step, -step),
        Vec2::new(2.0 * step, 0.0),
        Vec2::new(0.0, 2.0 * step),
        Vec2::new(-2.0 * step, 0.0),
        Vec2::new(0.0, -2.0 * step),
    ]
}

#[must_use]
pub(crate) fn structure_timer_frames(
    base_seconds: f32,
    wealthy_rate: f32,
    poor_rate: f32,
    wealthy_threshold: i64,
    poor_threshold: i64,
    purse: i64,
) -> u32 {
    let rate = if purse >= wealthy_threshold && wealthy_rate > 0.0 {
        wealthy_rate
    } else if purse <= poor_threshold && poor_rate > 0.0 {
        poor_rate
    } else {
        1.0
    };
    let seconds = if base_seconds > 0.0 {
        base_seconds / rate
    } else {
        0.0
    };
    mint_sim::LogicFrames::from_seconds(seconds.max(0.0)).get()
}

#[must_use]
pub(crate) fn rebuild_ready(at: Frame, now: Frame, delay: mint_sim::LogicFrames) -> bool {
    now.get().saturating_sub(at.get()) >= u64::from(delay.get())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ArmyMemberPlan {
    pub(crate) kind: Option<Kind>,
    pub(crate) phase: [f32; 3],
}

#[must_use]
pub(crate) fn army_phase(elapsed_seconds: f32, rush: f32, mid_game: f32) -> u8 {
    let rush = if rush.is_finite() { rush.max(0.0) } else { 0.0 };
    let mid_game = if mid_game.is_finite() {
        mid_game.max(0.0)
    } else {
        0.0
    };
    if elapsed_seconds < rush {
        1
    } else if elapsed_seconds < rush + mid_game {
        2
    } else {
        3
    }
}

#[must_use]
pub(crate) fn deviation_order(members: &[ArmyMemberPlan], counts: &[u32], phase: u8) -> Vec<usize> {
    let phase_index = usize::from(phase.saturating_sub(1)).min(2);
    let total: u32 = members
        .iter()
        .zip(counts)
        .filter(|(member, _)| member.kind.is_some())
        .map(|(_, count)| *count)
        .fold(0u32, u32::saturating_add);
    let mut ranked: Vec<(usize, f64)> = members
        .iter()
        .zip(counts)
        .enumerate()
        .filter(|(_, (member, _))| member.kind.is_some())
        .filter_map(|(index, (member, count))| {
            let target = *member.phase.get(phase_index)?;
            if target <= 0.0 {
                return None;
            }
            #[allow(clippy::cast_precision_loss)]
            let share = if total > 0 {
                f64::from(*count) * 100.0 / f64::from(total)
            } else {
                0.0
            };
            Some((index, share - f64::from(target)))
        })
        .collect();
    ranked.sort_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ranked.into_iter().map(|(index, _)| index).collect()
}

#[must_use]
pub(crate) fn below_command_point_floor(used_and_queued: i32, cap: i32, floor: f32) -> bool {
    if cap <= 0 {
        return false;
    }
    f64::from(used_and_queued) < f64::from(cap) * f64::from(floor.clamp(0.0, 1.0))
}

#[must_use]
pub(crate) fn spendable_purse(purse: i64, percent_to_save: f32, must_spend: bool) -> i64 {
    if must_spend || purse <= 0 {
        return purse.max(0);
    }
    let save = f64::from(percent_to_save.clamp(0.0, 1.0));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let held = (purse as f64 * save) as i64;
    purse.saturating_sub(held).max(0)
}

const SECOND_CHOICE_GAP: f32 = 1000.0;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct EnemyManager {
    pub(crate) candidates: Vec<PlayerId>,
    pub(crate) target: Option<PlayerId>,
}

impl EnemyManager {
    pub(crate) fn update(
        &mut self,
        defeated: impl Fn(PlayerId) -> bool,
        distance: impl Fn(PlayerId) -> f32,
        roll: impl FnOnce() -> u32,
    ) -> bool {
        let before = self.target;
        let mut lost_target = self.target.is_none();
        self.candidates.retain(|&player| {
            let gone = defeated(player);
            if gone && Some(player) == self.target {
                lost_target = true;
            }
            !gone
        });
        if self.candidates.is_empty() {
            self.target = None;
        } else if lost_target {
            let mut ranked: Vec<(f32, PlayerId)> = self
                .candidates
                .iter()
                .map(|&player| (distance(player), player))
                .collect();
            ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.get().cmp(&b.1.get())));
            self.target = match ranked.as_slice() {
                [first, second, ..] => {
                    let second_half = roll() <= 50;
                    Some(if second_half && second.0 - first.0 <= SECOND_CHOICE_GAP {
                        second.1
                    } else {
                        first.1
                    })
                }
                [only] => Some(only.1),
                [] => None,
            };
        }
        self.target != before
    }
}

#[must_use]
pub(crate) fn enemy_groups(
    players: &[PlayerId],
    allied: impl Fn(PlayerId, PlayerId) -> bool,
) -> Vec<Vec<PlayerId>> {
    let mut groups: Vec<Vec<PlayerId>> = Vec::new();
    for &player in players {
        match groups
            .iter_mut()
            .find(|group| group.first().is_some_and(|&lead| allied(player, lead)))
        {
            Some(group) => group.push(player),
            None => groups.push(vec![player]),
        }
    }
    groups
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteCategory {
    Center,
    Flank,
    Backdoor,
}

pub(crate) const ROUTE_CATEGORIES: [RouteCategory; 3] = [
    RouteCategory::Center,
    RouteCategory::Flank,
    RouteCategory::Backdoor,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RouteMarker {
    pub(crate) category: RouteCategory,
    pub(crate) position: Vec2,
}

#[must_use]
pub(crate) fn route_markers(objects: &[mint_sage_map::Object]) -> Vec<RouteMarker> {
    objects
        .iter()
        .filter_map(|object| {
            let lower = object.template_name.to_ascii_lowercase();
            let category = if lower.starts_with("center") {
                RouteCategory::Center
            } else if lower.starts_with("flank") {
                RouteCategory::Flank
            } else if lower.starts_with("backdoor") {
                RouteCategory::Backdoor
            } else {
                return None;
            };
            Some(RouteMarker {
                category,
                position: Vec2::new(object.x, object.y),
            })
        })
        .collect()
}

#[must_use]
pub(crate) fn pick_route(markers: &[RouteMarker], base_centre: Vec2, roll: u32) -> Option<Vec2> {
    let mut present: Vec<RouteCategory> = Vec::new();
    for category in ROUTE_CATEGORIES {
        if markers.iter().any(|marker| marker.category == category) {
            present.push(category);
        }
    }
    if present.is_empty() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    let index = (roll as usize) % present.len();
    let &chosen = present.get(index)?;
    markers
        .iter()
        .filter(|marker| marker.category == chosen)
        .min_by(|a, b| {
            let da = a.position.minus(base_centre).length_squared();
            let db = b.position.minus(base_centre).length_squared();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|marker| marker.position)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttackUrgency {
    pub(crate) attack: i32,
    pub(crate) defense: i32,
    pub(crate) economy: i32,
    pub(crate) armed: bool,
}

impl AttackUrgency {
    #[must_use]
    pub(crate) fn new(values: &crate::combat::AttackUrgencyValues) -> Self {
        Self {
            attack: values.attack_initial,
            defense: values.defense_initial,
            economy: values.economy_initial,
            armed: false,
        }
    }
}

#[must_use]
fn raise_attack_med(
    values: &crate::combat::AttackUrgencyValues,
    attack: i32,
    defense: i32,
    economy: i32,
) -> Option<(i32, i32, i32)> {
    let half = values.change_med / 2;
    let upper = values.upper_bound_med_base - values.change_med;
    if attack < upper && defense > half && economy > half {
        Some((attack + values.change_med, defense - half, economy - half))
    } else {
        None
    }
}

#[must_use]
fn lower_attack_high(
    values: &crate::combat::AttackUrgencyValues,
    attack: i32,
    defense: i32,
    economy: i32,
) -> Option<(i32, i32, i32)> {
    let half = values.change_high / 2;
    let upper = values.upper_bound_high_base - values.change_high;
    if attack > values.change_high && defense < upper && economy < upper {
        Some((attack - values.change_high, defense + half, economy + half))
    } else {
        None
    }
}

#[must_use]
fn launch_reset(
    values: &crate::combat::AttackUrgencyValues,
    mut attack: i32,
    mut defense: i32,
    mut economy: i32,
) -> (i32, i32, i32) {
    for _ in 0..4 {
        match lower_attack_high(values, attack, defense, economy) {
            Some((a, d, e)) => {
                attack = a;
                defense = d;
                economy = e;
            }
            None => break,
        }
    }
    (attack, defense, economy)
}

#[must_use]
pub(crate) const fn command_points_low(available: i32, threshold: i32) -> bool {
    available <= threshold
}

#[must_use]
pub(crate) fn advance_attack_urgency(
    values: &crate::combat::AttackUrgencyValues,
    mut urgency: AttackUrgency,
    command_points_low: bool,
) -> (AttackUrgency, bool) {
    if command_points_low {
        if let Some((attack, defense, economy)) =
            raise_attack_med(values, urgency.attack, urgency.defense, urgency.economy)
        {
            urgency.attack = attack;
            urgency.defense = defense;
            urgency.economy = economy;
        }
    }
    let crossed = !urgency.armed && urgency.attack >= values.launch;
    if crossed {
        urgency.armed = true;
        let (attack, defense, economy) =
            launch_reset(values, urgency.attack, urgency.defense, urgency.economy);
        urgency.attack = attack;
        urgency.defense = defense;
        urgency.economy = economy;
    } else if urgency.attack < values.launch {
        urgency.armed = false;
    }
    (urgency, crossed)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AttackPhase {
    EnRoute { marker: Vec2 },
    Assaulting { target: Entity },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AttackWave {
    pub(crate) phase: AttackPhase,
    pub(crate) hordes: Vec<Entity>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AiAttackProgress {
    pub(crate) urgency: AttackUrgency,
    pub(crate) check_timer: u32,
    pub(crate) lead: PlayerId,
    pub(crate) enemies: Option<EnemyManager>,
    pub(crate) wave: Option<AttackWave>,
    pub(crate) launches: u32,
}

impl AiAttackProgress {
    #[must_use]
    pub(crate) fn new(
        values: &crate::combat::AttackUrgencyValues,
        lead: PlayerId,
        enemies: Option<EnemyManager>,
    ) -> Self {
        Self {
            urgency: AttackUrgency::new(values),
            check_timer: 0,
            lead,
            enemies,
            wave: None,
            launches: 0,
        }
    }
}

impl Component for AiAttackProgress {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.urgency.attack);
        hasher.write_i32(self.urgency.defense);
        hasher.write_i32(self.urgency.economy);
        hasher.write_bool(self.urgency.armed);
        hasher.write_u32(self.check_timer);
        self.lead.hash_into(hasher);
        match &self.enemies {
            Some(manager) => {
                hasher.write_u8(1);
                hasher.write_usize(manager.candidates.len());
                for player in &manager.candidates {
                    player.hash_into(hasher);
                }
                match manager.target {
                    Some(player) => {
                        hasher.write_u8(1);
                        player.hash_into(hasher);
                    }
                    None => hasher.write_u8(0),
                }
            }
            None => hasher.write_u8(0),
        }
        match &self.wave {
            None => hasher.write_u8(0),
            Some(wave) => {
                hasher.write_u8(1);
                match wave.phase {
                    AttackPhase::EnRoute { marker } => {
                        hasher.write_u8(0);
                        hasher.write_f32(marker.x);
                        hasher.write_f32(marker.y);
                    }
                    AttackPhase::Assaulting { target } => {
                        hasher.write_u8(1);
                        target.hash_into(hasher);
                    }
                }
                hasher.write_usize(wave.hordes.len());
                for horde in &wave.hordes {
                    horde.hash_into(hasher);
                }
            }
        }
        hasher.write_u32(self.launches);
    }
}

#[cfg(test)]
mod tests;
