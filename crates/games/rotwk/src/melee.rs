use mint_movement::{Facing, Vec2};
use mint_sim::{Component, Entity, Frame, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeleePhase {
    Approach,
    Wait { until: Frame },
    WaitPath { until: Frame, checks: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeleeKind {
    Swarm,
    HoldGround,
    Amoeba,
    Formation,
}

impl MeleeKind {
    pub(crate) fn of(def: &mint_sage_ini::MeleeBehaviorDef) -> Self {
        use mint_sage_ini::MeleeBehaviorKind as Named;
        match def.kind {
            Named::Amoeba => Self::Amoeba,
            Named::HoldGround => Self::HoldGround,
            Named::Swarm => Self::Swarm,
            Named::WaitForLeader => Self::Formation,
        }
    }

    const fn tag(self) -> u8 {
        match self {
            Self::Swarm => 0,
            Self::HoldGround => 1,
            Self::Amoeba => 2,
            Self::Formation => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HordeMelee {
    pub(crate) goal: Entity,
    pub(crate) kind: MeleeKind,
    pub(crate) phase: MeleePhase,
    pub(crate) ready_until: Option<Frame>,
    pub(crate) victim: Option<Entity>,
    pub(crate) force: bool,
    pub(crate) planned: Option<(Vec2, Frame)>,
}

impl HordeMelee {
    pub(crate) const fn new(goal: Entity, kind: MeleeKind) -> Self {
        Self {
            goal,
            kind,
            phase: MeleePhase::Approach,
            ready_until: None,
            victim: None,
            force: false,
            planned: None,
        }
    }
}

impl Component for HordeMelee {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.goal.hash_into(hasher);
        hasher.write_u8(self.kind.tag());
        match self.phase {
            MeleePhase::Approach => hasher.write_u8(0),
            MeleePhase::Wait { until } => {
                hasher.write_u8(1);
                until.hash_into(hasher);
            }
            MeleePhase::WaitPath { until, checks } => {
                hasher.write_u8(2);
                until.hash_into(hasher);
                hasher.write_u8(checks);
            }
        }
        match self.ready_until {
            Some(until) => {
                hasher.write_u8(1);
                until.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.victim {
            Some(victim) => {
                hasher.write_u8(1);
                victim.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_bool(self.force);
        match self.planned {
            Some((at, when)) => {
                hasher.write_u8(1);
                hasher.write_f32(at.x);
                hasher.write_f32(at.y);
                when.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
    }
}

pub(crate) const HISTORY: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwarmState {
    None,
    Moving,
    Rotating,
    Arrived,
}

impl SwarmState {
    const fn tag(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Moving => 1,
            Self::Rotating => 2,
            Self::Arrived => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MemberMelee {
    pub(crate) goal: Vec2,
    pub(crate) face: Vec2,
    history: [u32; HISTORY],
    filled: u8,
    next: u8,
    pub(crate) idle: bool,
    pub(crate) timer: u32,
    pub(crate) state: SwarmState,
    pub(crate) wait_until: Option<Frame>,
    pub(crate) dirty: bool,
}

impl MemberMelee {
    pub(crate) const fn at(at: Vec2, delay_until_idle: u32) -> Self {
        Self {
            goal: at,
            face: at,
            history: [0; HISTORY],
            filled: 0,
            next: 0,
            idle: false,
            timer: delay_until_idle,
            state: SwarmState::None,
            wait_until: None,
            dirty: false,
        }
    }

    pub(crate) fn remembers(&self, cell: u32) -> bool {
        self.history
            .iter()
            .take(usize::from(self.filled))
            .any(|seen| *seen == cell)
    }

    pub(crate) fn remember(&mut self, cell: u32) {
        if let Some(slot) = self.history.get_mut(usize::from(self.next)) {
            *slot = cell;
        }
        #[allow(clippy::cast_possible_truncation)]
        let len = HISTORY as u8;
        self.next = self.next.saturating_add(1) % len;
        self.filled = self.filled.saturating_add(1).min(len);
    }

    pub(crate) fn forget(&mut self) {
        self.filled = 0;
        self.next = 0;
    }
}

impl Component for MemberMelee {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.goal.x);
        hasher.write_f32(self.goal.y);
        hasher.write_f32(self.face.x);
        hasher.write_f32(self.face.y);
        hasher.write_u8(self.filled);
        hasher.write_u8(self.next);
        for cell in self.history.iter().take(usize::from(self.filled)) {
            hasher.write_u32(*cell);
        }
        hasher.write_bool(self.idle);
        hasher.write_u32(self.timer);
        hasher.write_u8(self.state.tag());
        match self.wait_until {
            Some(until) => {
                hasher.write_u8(1);
                until.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_bool(self.dirty);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdleChange {
    None,
    Start,
    Stop,
}

impl MemberMelee {
    pub(crate) fn tick_idle(&mut self, delay_until_idle: u32, activate: u32) -> IdleChange {
        if self.timer > 0 {
            self.timer -= 1;
            return IdleChange::None;
        }
        if self.idle {
            self.idle = false;
            self.timer = delay_until_idle;
            IdleChange::Stop
        } else {
            self.idle = true;
            self.timer = activate;
            IdleChange::Start
        }
    }

    pub(crate) fn stop_idle(&mut self, delay_until_idle: u32) -> bool {
        let was = self.idle;
        self.idle = false;
        self.timer = delay_until_idle;
        was
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MemberAttack {
    pub(crate) victim: Entity,
    pub(crate) out_of_reach_since: Option<Frame>,
    pub(crate) spot: Option<Vec2>,
    pub(crate) planned: Option<(Vec2, Frame)>,
    pub(crate) waits_until: Option<Frame>,
}

impl MemberAttack {
    pub(crate) const fn on(victim: Entity) -> Self {
        Self {
            victim,
            out_of_reach_since: None,
            spot: None,
            planned: None,
            waits_until: None,
        }
    }
}

impl Component for MemberAttack {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.victim.hash_into(hasher);
        match self.out_of_reach_since {
            Some(since) => {
                hasher.write_u8(1);
                since.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.spot {
            Some(spot) => {
                hasher.write_u8(1);
                hasher.write_f32(spot.x);
                hasher.write_f32(spot.y);
            }
            None => hasher.write_u8(0),
        }
        match self.planned {
            Some((seen, when)) => {
                hasher.write_u8(1);
                hasher.write_f32(seen.x);
                hasher.write_f32(seen.y);
                when.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.waits_until {
            Some(until) => {
                hasher.write_u8(1);
                until.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
    }
}

pub(crate) const GIVE_UP_MS: u32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AmoebaTuning {
    pub(crate) facing_bonus: f32,
    pub(crate) angle_limit_cos: f32,
    pub(crate) inner_range: f32,
    pub(crate) outer_range: f32,
    pub(crate) outer_range_buildings: f32,
    pub(crate) delay_until_idle: u32,
    pub(crate) activate_min: u32,
    pub(crate) activate_max: u32,
}

impl AmoebaTuning {
    pub(crate) fn of(def: &mint_sage_ini::AmoebaDef) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = |ms: f32| {
            (ms / 1000.0 * crate::game_data::RETAIL_LOGIC_HZ as f32)
                .ceil()
                .clamp(0.0, 1.0e6) as u32
        };
        Self {
            facing_bonus: def.facing_bonus,
            angle_limit_cos: def.angle_limit_cos,
            inner_range: def.inner_range,
            outer_range: def.outer_range,
            outer_range_buildings: def.outer_range_buildings,
            delay_until_idle: frames(def.delay_until_idle_ms),
            activate_min: frames(def.delay_random_activate_min_ms),
            activate_max: frames(def.delay_random_activate_max_ms),
        }
    }
}

pub(crate) const BASE_SCORE: f32 = 10_000.0;

#[allow(clippy::approx_constant)]
const RETAIL_PI: f32 = 3.141;

pub(crate) const RIM_SCORE: f32 = 1_000_000.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Step {
    pub(crate) from: Vec2,
    pub(crate) facing: Facing,
    pub(crate) radius: f32,
    pub(crate) to: Vec2,
    pub(crate) enemy: Vec2,
    pub(crate) enemy_radius: f32,
    pub(crate) enemy_is_structure: bool,
    pub(crate) centre: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Weigh {
    Score(f32),
    OutsideCone,
    BeyondOuter,
}

pub(crate) fn weigh(step: &Step, tuning: &AmoebaTuning) -> Weigh {
    let travel = step.to.minus(step.from);
    let turn = Facing::from_vector(travel).map_or(0.0, |towards| step.facing.delta_to(towards));
    let mut score = BASE_SCORE - turn.abs() * tuning.facing_bonus / RETAIL_PI;
    let back = step.from.minus(step.to);
    let toward_me = step.from.minus(step.enemy).normalised();
    let inside = back
        .normalised()
        .zip(toward_me)
        .is_none_or(|(along, toward_me)| {
            let side = Vec2::new(-along.y, along.x).scale(step.enemy_radius);
            let edge = |sign: f32| {
                back.plus(side.scale(sign))
                    .normalised()
                    .map_or(1.0, |edge| edge.dot(toward_me))
            };
            edge(1.0) >= tuning.angle_limit_cos || edge(-1.0) >= tuning.angle_limit_cos
        });
    if !inside {
        return Weigh::OutsideCone;
    }
    let gap = (step.to.minus(step.enemy).length() - step.radius - step.enemy_radius).max(0.0);
    score -= gap;
    let from_centre = step.to.minus(step.centre).length();
    let outer = if step.enemy_is_structure {
        tuning.outer_range_buildings
    } else {
        tuning.outer_range
    };
    if from_centre > outer {
        return Weigh::BeyondOuter;
    }
    if from_centre > tuning.inner_range {
        score -= from_centre;
    }
    Weigh::Score(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuning() -> AmoebaTuning {
        AmoebaTuning::of(&mint_sage_ini::AmoebaDef::default())
    }

    fn step(to: Vec2) -> Step {
        Step {
            from: Vec2::ZERO,
            facing: Facing::from_radians(0.0),
            radius: 8.0,
            to,
            enemy: Vec2::new(40.0, 0.0),
            enemy_radius: 8.0,
            enemy_is_structure: false,
            centre: Vec2::ZERO,
        }
    }

    #[test]
    fn the_amoeba_scores_a_cell_toward_the_enemy_and_refuses_away_or_past_outer_range() {
        let toward = weigh(&step(Vec2::new(10.0, 0.0)), &tuning());
        let aside = weigh(&step(Vec2::new(10.0, 10.0)), &tuning());
        let (Weigh::Score(toward), Weigh::Score(aside)) = (toward, aside) else {
            panic!("both steps are inside the cone: {toward:?} {aside:?}");
        };
        assert!((toward - (BASE_SCORE - 14.0)).abs() < 1e-3, "{toward}");
        assert!(aside < toward, "a turn and a longer gap cost");
        assert_eq!(
            weigh(&step(Vec2::new(-10.0, 0.0)), &tuning()),
            Weigh::OutsideCone
        );
        let mut far = step(Vec2::new(10.0, 0.0));
        far.centre = Vec2::new(-85.0, 0.0);
        assert_eq!(weigh(&far, &tuning()), Weigh::BeyondOuter);
        far.centre = Vec2::new(-65.0, 0.0);
        let Weigh::Score(pulled) = weigh(&far, &tuning()) else {
            panic!("inside OuterRange");
        };
        assert!((pulled - (toward - 75.0)).abs() < 1e-3, "past InnerRange");
    }

    #[test]
    fn the_idle_timers_alternate_between_delays() {
        let mut record = MemberMelee::at(Vec2::ZERO, 2);
        let changes: Vec<IdleChange> = (0..7).map(|_| record.tick_idle(2, 1)).collect();
        assert_eq!(
            changes,
            [
                IdleChange::None,
                IdleChange::None,
                IdleChange::Start,
                IdleChange::None,
                IdleChange::Stop,
                IdleChange::None,
                IdleChange::None,
            ]
        );
    }
}
