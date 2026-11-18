use mint_movement::{
    AvoidanceMode, Facing, MotionLimits, MoverState, Route, SearchMode, SmoothingMode, Vec2,
};

use crate::horde::FormationMode;
use mint_sage_combat::{ArmorId, WeaponId};
use mint_sim::{Component, LogicFrames, PlayerId, StateHasher};

use mint_sage_asset::{KindOf, PickType};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Body(pub(crate) MoverState);

impl Component for Body {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0.position.x);
        hasher.write_f32(self.0.position.y);
        hasher.write_f32(self.0.facing.radians());
        hasher.write_f32(self.0.speed);
        hasher.write_u32(self.0.waypoint);
        hasher.write_u32(u32::from(self.0.ramp.bits()));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Motive(pub(crate) mint_movement::MotionPhase);

impl Component for Motive {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(u32::from(self.0.bits()));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DesiredSpeed(pub(crate) f32);

impl Component for DesiredSpeed {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0.to_bits());
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FormationSpeed {
    pub(crate) speed: f32,
    pub(crate) group: CommandGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandGroup {
    pub(crate) issued: mint_sim::Frame,
    pub(crate) first: mint_sim::Entity,
}

impl Component for FormationSpeed {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.speed.to_bits());
        hasher.write_u64(self.group.issued.get());
        hasher.write_u32(self.group.first.index());
        hasher.write_u32(self.group.first.generation());
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Locomotion(pub(crate) Kinematics);

impl Component for Locomotion {
    fn hash_into(&self, hasher: &mut StateHasher) {
        let limits = self.0.limits;
        hasher.write_f32(limits.max_speed);
        hasher.write_f32(limits.acceleration);
        hasher.write_f32(limits.braking);
        hasher.write_f32(limits.turn_rate);
        hasher.write_f32(limits.arrival_radius);
        hasher.write_f32(limits.turn_threshold);
        hasher.write_f32(limits.min_speed);
        hasher.write_f32(limits.walk_distance);
        hasher.write_f32(limits.corner_radius);
        hasher.write_u32(
            u32::from(limits.turn_while_moving) | u32::from(limits.turn_phases_at_speed) << 1,
        );
        hasher.write_f32(self.0.damaged_turn_rate);
        hasher.write_f32(self.0.damaged_speed);
        hasher.write_f32(self.0.look_ahead_mult);
        hasher.write_f32(self.0.max_turn_without_reform.unwrap_or(-1.0));
        hasher.write_f32(self.0.charge_speed);
        hasher.write_u32(
            u32::from(self.0.charge_available)
                | u32::from(self.0.charge_ignores_condition) << 1
                | u32::from(self.0.crew_powered) << 2
                | u32::from(self.0.scales_walls) << 3,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Kinematics {
    pub(crate) limits: MotionLimits,
    pub(crate) damaged_turn_rate: f32,
    pub(crate) damaged_speed: f32,
    pub(crate) look_ahead_mult: f32,
    pub(crate) max_turn_without_reform: Option<f32>,
    pub(crate) charge_speed: f32,
    pub(crate) charge_available: bool,
    pub(crate) charge_ignores_condition: bool,
    pub(crate) departure: DepartureRadii,
    pub(crate) turn_pivot_offset: f32,
    pub(crate) backs_in_terror: bool,
    pub(crate) crew_powered: bool,
    pub(crate) scales_walls: bool,
    pub(crate) wander_radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DepartureRadii {
    pub(crate) slow: f32,
    pub(crate) fast: f32,
    pub(crate) max_turn: f32,
    pub(crate) max_turn_fast: f32,
}

#[cfg(test)]
impl DepartureRadii {
    pub(crate) const NONE: Self = Self {
        slow: 0.0,
        fast: 0.0,
        max_turn: 0.0,
        max_turn_fast: 0.0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChargeState {
    pub(crate) attacking: bool,
    pub(crate) ability: bool,
}

impl Kinematics {
    pub(crate) fn at_health(self, health: Option<f32>, penalty: f32) -> MotionLimits {
        self.at_health_charging(health, penalty, None, 1.0)
    }

    pub(crate) fn at_health_charging(
        self,
        health: Option<f32>,
        penalty: f32,
        charge: Option<ChargeState>,
        crew_factor: f32,
    ) -> MotionLimits {
        let damaged = health.is_some_and(|fraction| fraction <= penalty);
        let limits = if damaged {
            MotionLimits {
                turn_rate: self.damaged_turn_rate,
                max_speed: self.limits.max_speed * self.damaged_speed,
                acceleration: self.limits.acceleration * self.damaged_speed,
                braking: self.limits.braking * self.damaged_speed,
                ..self.limits
            }
        } else {
            self.limits
        };
        let charging =
            charge.is_some_and(|state| (self.charge_available && state.attacking) || state.ability);
        let limits = if charging && (!damaged || self.charge_ignores_condition) {
            MotionLimits {
                max_speed: limits.max_speed * self.charge_speed,
                ..limits
            }
        } else {
            limits
        };
        if self.crew_powered {
            MotionLimits {
                max_speed: limits.max_speed * crew_factor,
                ..limits
            }
        } else {
            limits
        }
    }
}

pub(crate) fn current_limits(
    world: &mint_sim::World,
    kinds: &crate::templates::KindTable,
    ids: (
        mint_sim::ComponentId<Kind>,
        mint_sim::ComponentId<Locomotion>,
        mint_sim::ComponentId<crate::combat::Health>,
    ),
    entity: mint_sim::Entity,
    charge: Option<ChargeState>,
    crew_factor: f32,
) -> Option<MotionLimits> {
    let (kind_id, locomotion_id, health_id) = ids;
    let entry = world
        .get(kind_id, entity)
        .copied()
        .and_then(|kind| kinds.get(kind));
    let kinematics = world
        .get(locomotion_id, entity)
        .map(|locomotion| locomotion.0)
        .or_else(|| entry.and_then(|entry| entry.limits))?;
    let health = world
        .get(health_id, entity)
        .map(|health| health.0.fraction());
    let penalty = entry.map_or(f32::NEG_INFINITY, |entry| entry.movement_penalty);
    Some(kinematics.at_health_charging(health, penalty, charge, crew_factor))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Shape {
    pub(crate) kind_of: KindOf,
    pub(crate) radius: f32,
    pub(crate) height: f32,
    pub(crate) geometry_is_small: bool,
}

impl Shape {
    pub(crate) const fn pick_type(&self) -> PickType {
        self.kind_of.pick_type()
    }
}

impl Component for Shape {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.kind_of.bits());
        hasher.write_f32(self.radius);
        hasher.write_f32(self.height);
        hasher.write_bool(self.geometry_is_small);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GeometryFrom(pub(crate) Kind);

impl Component for GeometryFrom {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MapPlacement(pub(crate) u32);

impl Component for MapPlacement {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GroundOffset(pub(crate) f32);

impl Component for GroundOffset {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Elevation(pub(crate) f32, pub(crate) Option<StoodStill>);

impl Elevation {
    pub(crate) const fn new(z: f32) -> Self {
        Self(z, None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StoodStill {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) z: u32,
    pub(crate) layer: crate::wall_layer::Layer,
    pub(crate) kind: Kind,
    pub(crate) walls: Option<u64>,
    pub(crate) horde: Option<mint_sim::Entity>,
    pub(crate) horde_layer: Option<crate::wall_layer::Layer>,
    pub(crate) glided: bool,
    pub(crate) path: bool,
}

impl Component for Elevation {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Owner(pub(crate) PlayerId);

impl Component for Owner {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selected(pub(crate) u8);

impl Selected {
    pub(crate) fn bit(player: PlayerId) -> Option<u8> {
        (player.get() < 8).then(|| 1u8 << player.get())
    }

    pub(crate) fn holds(self, player: PlayerId) -> bool {
        Self::bit(player).is_some_and(|bit| self.0 & bit != 0)
    }
}

impl Component for Selected {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathKind {
    Closing = 0,
    Ordered = 1,
    Ability = 2,
    Burning = 3,
    Formation = 4,
    Emotion = 5,
    Animal = 6,
    Amphibious = 7,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Path {
    pub(crate) route: Route,
    pub(crate) goal: Vec2,
    pub(crate) kind: PathKind,
    pub(crate) facing: Option<Facing>,
}

impl Component for Path {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.route.points.len());
        for point in &self.route.points {
            hasher.write_f32(point.x);
            hasher.write_f32(point.y);
        }
        hasher.write_f32(self.goal.x);
        hasher.write_f32(self.goal.y);
        hasher.write_u8(self.kind as u8);
        hasher.write_bool(self.facing.is_some());
        hasher.write_f32(self.facing.map_or(0.0, Facing::radians));
    }
}

pub(crate) fn give_path(
    world: &mut mint_sim::World,
    (bodies, paths): (mint_sim::ComponentId<Body>, mint_sim::ComponentId<Path>),
    entity: mint_sim::Entity,
    path: Path,
) -> mint_sim::Result<()> {
    if let Some(mut body) = world.get(bodies, entity).copied() {
        body.0.waypoint = 0;
        world.insert(bodies, entity, body)?;
    }
    world.insert(paths, entity, path)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Modes {
    pub(crate) search: SearchMode,
    pub(crate) smoothing: SmoothingMode,
    pub(crate) avoidance: AvoidanceMode,
    pub(crate) formation: FormationMode,
}

impl Modes {
    pub(crate) const fn config(self) -> mint_movement::PipelineConfig {
        mint_movement::PipelineConfig {
            search: self.search,
            smoothing: self.smoothing,
            avoidance: self.avoidance,
        }
    }
}

fn mode_codes(modes: Modes) -> (u8, u8, u8, u8) {
    let search = match modes.search {
        SearchMode::StraightLine => 0,
        SearchMode::AStar => 1,
    };
    let smoothing = match modes.smoothing {
        SmoothingMode::Off => 0,
        SmoothingMode::StringPull => 1,
    };
    let avoidance = match modes.avoidance {
        AvoidanceMode::Off => 0,
        AvoidanceMode::Separation => 1,
    };
    let formation = match modes.formation {
        FormationMode::On => 0,
        FormationMode::Off => 1,
    };
    (search, smoothing, avoidance, formation)
}

impl Component for Modes {
    fn hash_into(&self, hasher: &mut StateHasher) {
        let (search, smoothing, avoidance, formation) = mode_codes(*self);
        hasher.write_u8(search);
        hasher.write_u8(smoothing);
        hasher.write_u8(avoidance);
        hasher.write_u8(formation);
    }
}

pub(crate) fn hash_modes(modes: Modes, hasher: &mut StateHasher) {
    modes.hash_into(hasher);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Kind(pub(crate) u16);

impl Component for Kind {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u16(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spawn {
    pub(crate) position: Vec2,
    pub(crate) facing: Facing,
    pub(crate) shape: Shape,
    pub(crate) owner: PlayerId,
    pub(crate) health: Option<f32>,
    pub(crate) initial_health: Option<f32>,
    pub(crate) weapon: Option<WeaponId>,
    pub(crate) armour: Option<ArmorId>,
    pub(crate) kind: Option<Kind>,
    pub(crate) vision: Option<f32>,
    pub(crate) producer: Option<ProducerSpawn>,
    pub(crate) income: Option<(LogicFrames, i64)>,
    pub(crate) claim: Option<crate::claims::Claimant>,
    pub(crate) upkeep: Option<i32>,
    pub(crate) ground_offset: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProducerSpawn {
    pub(crate) menu: u16,
    pub(crate) rally: Vec2,
    pub(crate) depth: u32,
}

impl Spawn {
    pub(crate) const fn inert(
        position: Vec2,
        facing: Facing,
        shape: Shape,
        owner: PlayerId,
    ) -> Self {
        Self {
            position,
            facing,
            shape,
            owner,
            health: None,
            initial_health: None,
            weapon: None,
            armour: None,
            kind: None,
            vision: None,
            producer: None,
            income: None,
            claim: None,
            upkeep: None,
            ground_offset: None,
        }
    }

    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.position.x);
        hasher.write_f32(self.position.y);
        hasher.write_f32(self.facing.radians());
        self.shape.hash_into(hasher);
        hasher.write_u8(self.owner.get());
        hasher.write_u32(self.health.map_or(u32::MAX, f32::to_bits));
        match self.weapon {
            Some(id) => {
                hasher.write_u8(1);
                id.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.armour {
            Some(id) => {
                hasher.write_u8(1);
                id.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.kind {
            Some(kind) => {
                hasher.write_u8(1);
                kind.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_u32(self.vision.map_or(u32::MAX, f32::to_bits));
        hasher.write_u8(u8::from(self.producer.is_some()));
        hasher.write_u8(u8::from(self.income.is_some()));
        hasher.write_u8(u8::from(self.claim.is_some()));
        hasher.write_u8(u8::from(self.ground_offset.is_some()));
        hasher.write_i64(self.upkeep.map_or(i64::from(i32::MIN), i64::from));
        if let Some(initial) = self.initial_health {
            hasher.write_f32(initial);
        }
    }
}

pub(crate) const PLACEHOLDER_LIMITS: MotionLimits = MotionLimits {
    turn_pivot: 0.0,
    backs: false,
    desired_speed: f32::INFINITY,
    stops_dead: false,
    max_speed: 40.0,
    acceleration: 60.0,
    braking: 80.0,
    turn_rate: 10.0,
    arrival_radius: 5.0,
    turn_threshold: 0.0,
    acc_dec_trigger: 0.5,
    min_speed: 0.0,
    turn_circle: None,
    turn_slowdown: mint_movement::TurnSlowdown::PROPORTIONAL,
    frame: crate::templates::RETAIL_FRAME_SECONDS,
    turn_while_moving: true,
    walk_distance: 0.0,
    turn_phases_at_speed: false,
    corner_radius: 5.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_damaged_turn_rate_applies_from_the_penalty_state_down() {
        let kinematics = Kinematics {
            limits: PLACEHOLDER_LIMITS,
            damaged_turn_rate: 1.0,
            damaged_speed: 1.0,
            look_ahead_mult: 1.0,
            max_turn_without_reform: None,
            charge_speed: 1.0,
            charge_available: false,
            charge_ignores_condition: false,
            departure: DepartureRadii::NONE,
            turn_pivot_offset: 0.0,
            backs_in_terror: false,
            crew_powered: false,
            scales_walls: false,
            wander_radius: 0.0,
        };
        let rate = |health| kinematics.at_health(health, 0.4).turn_rate;
        assert_eq!(
            [
                rate(Some(0.41)),
                rate(Some(0.4)),
                rate(Some(0.1)),
                rate(None)
            ],
            [
                PLACEHOLDER_LIMITS.turn_rate,
                1.0,
                1.0,
                PLACEHOLDER_LIMITS.turn_rate
            ]
        );
    }

    #[test]
    fn charge_speed_applies_while_charging_and_undamaged_or_ignoring_damage() {
        let kinematics = |available, ignores| Kinematics {
            limits: PLACEHOLDER_LIMITS,
            damaged_turn_rate: 1.0,
            damaged_speed: if ignores { 1.0 } else { 0.5 },
            look_ahead_mult: 1.0,
            max_turn_without_reform: None,
            charge_speed: 2.0,
            charge_available: available,
            charge_ignores_condition: ignores,
            departure: DepartureRadii::NONE,
            turn_pivot_offset: 0.0,
            backs_in_terror: false,
            crew_powered: false,
            scales_walls: false,
            wander_radius: 0.0,
        };
        let base = PLACEHOLDER_LIMITS.max_speed;
        let state = |attacking, ability| Some(ChargeState { attacking, ability });
        let speed = |k: Kinematics, health, charge| {
            k.at_health_charging(health, 0.4, charge, 1.0).max_speed
        };
        let rows = [
            (
                kinematics(true, false),
                Some(1.0),
                state(true, false),
                base * 2.0,
            ),
            (
                kinematics(false, false),
                Some(1.0),
                state(true, false),
                base,
            ),
            (
                kinematics(true, false),
                Some(1.0),
                state(false, false),
                base,
            ),
            (
                kinematics(false, false),
                Some(1.0),
                state(false, true),
                base * 2.0,
            ),
            (
                kinematics(true, false),
                Some(0.3),
                state(true, false),
                base * 0.5,
            ),
            (
                kinematics(true, true),
                Some(0.3),
                state(true, false),
                base * 2.0,
            ),
        ];
        for (row, (k, health, charge, want)) in rows.into_iter().enumerate() {
            assert_eq!(speed(k, health, charge), want, "row {row}");
        }
    }

    #[test]
    fn crew_factor_applies_only_when_crew_powered() {
        let kinematics = |crew_powered| Kinematics {
            limits: PLACEHOLDER_LIMITS,
            damaged_turn_rate: 1.0,
            damaged_speed: 1.0,
            look_ahead_mult: 1.0,
            max_turn_without_reform: None,
            charge_speed: 1.0,
            charge_available: false,
            charge_ignores_condition: false,
            departure: DepartureRadii::NONE,
            turn_pivot_offset: 0.0,
            backs_in_terror: false,
            crew_powered,
            scales_walls: false,
            wander_radius: 0.0,
        };
        let base = PLACEHOLDER_LIMITS.max_speed;
        let crew_factor = 6.0 * 0.5;
        assert_eq!(
            kinematics(true)
                .at_health_charging(None, 0.4, None, crew_factor)
                .max_speed,
            base * crew_factor
        );
        assert_eq!(
            kinematics(false)
                .at_health_charging(None, 0.4, None, crew_factor)
                .max_speed,
            base
        );
    }

    #[test]
    fn the_placeholder_locomotor_can_turn_inside_its_own_arrival_radius() {
        assert!(
            PLACEHOLDER_LIMITS.clears_turning_circle(),
            "turning radius {} against arrival radius {}",
            PLACEHOLDER_LIMITS.turning_radius(),
            PLACEHOLDER_LIMITS.arrival_radius
        );
    }

    #[test]
    fn a_units_template_changes_its_hash() {
        let hash_of = |kind: Kind| {
            let mut hasher = StateHasher::new();
            kind.hash_into(&mut hasher);
            hasher.finish()
        };
        assert_ne!(hash_of(Kind(0)), hash_of(Kind(1)));
        assert_ne!(hash_of(Kind(1)), hash_of(Kind(257)));
    }

    #[test]
    fn a_selection_mask_holds_one_bit_per_player_and_refuses_a_ninth() {
        let mask = Selected(0b0000_0101);
        assert!(mask.holds(PlayerId::new(0)));
        assert!(!mask.holds(PlayerId::new(1)));
        assert!(mask.holds(PlayerId::new(2)));
        assert_eq!(Selected::bit(PlayerId::new(8)), None);
        assert!(!mask.holds(PlayerId::new(8)));
    }

    #[test]
    fn every_field_of_a_body_and_a_shape_changes_its_hash() {
        let base = Body(MoverState::new(Vec2::new(1.0, 2.0), Facing::EAST));
        let hash_of = |body: &Body| {
            let mut hasher = StateHasher::new();
            body.hash_into(&mut hasher);
            hasher.finish()
        };
        let mut moved = base;
        moved.0.position.x = 1.5;
        let mut turned = base;
        turned.0.facing = Facing::from_radians(1.0);
        let mut faster = base;
        faster.0.speed = 3.0;
        let mut further = base;
        further.0.waypoint = 2;
        for other in [moved, turned, faster, further] {
            assert_ne!(hash_of(&base), hash_of(&other), "{other:?}");
        }

        let shape = Shape {
            kind_of: KindOf::SELECTABLE,
            radius: 5.0,
            height: 18.0,
            geometry_is_small: false,
        };
        let hash_shape = |shape: &Shape| {
            let mut hasher = StateHasher::new();
            shape.hash_into(&mut hasher);
            hasher.finish()
        };
        let mut wider = shape;
        wider.radius = 6.0;
        let mut taller = shape;
        taller.height = 19.0;
        let mut other_kind = shape;
        other_kind.kind_of = KindOf::SHRUBBERY;
        let mut small = shape;
        small.geometry_is_small = true;
        for other in [wider, taller, other_kind, small] {
            assert_ne!(hash_shape(&shape), hash_shape(&other), "{other:?}");
        }
    }

    #[test]
    fn a_paths_kind_changes_its_hash() {
        let hash_of = |path: &Path| {
            let mut hasher = StateHasher::new();
            path.hash_into(&mut hasher);
            hasher.finish()
        };
        let ordered = Path {
            route: Route::to_point(Vec2::new(1.0, 2.0)),
            goal: Vec2::new(1.0, 2.0),
            kind: PathKind::Ordered,
            facing: None,
        };
        let closing = Path {
            kind: PathKind::Closing,
            facing: None,
            ..ordered.clone()
        };
        assert_ne!(hash_of(&ordered), hash_of(&closing));
    }

    #[test]
    fn every_movement_mode_gets_its_own_code() {
        let mut codes = Vec::new();
        for search in SearchMode::ALL {
            for smoothing in SmoothingMode::ALL {
                for avoidance in AvoidanceMode::ALL {
                    for formation in FormationMode::ALL {
                        codes.push(mode_codes(Modes {
                            search,
                            smoothing,
                            avoidance,
                            formation,
                        }));
                    }
                }
            }
        }
        let mut unique = codes.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(codes.len(), unique.len(), "two mode sets share a code");
    }
}
