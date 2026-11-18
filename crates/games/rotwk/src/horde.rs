use std::collections::BTreeSet;

use mint_movement::Passability as _;
use mint_movement::{
    Cell, CellGeometry, CostGrid, Facing, HeightField, MotionPhase, MoverClass, MoverId,
    MoverState, Reachability, Stagger, Vec2,
};
use mint_sage_formation::{assign_slots, FormationBasis, FormationTable};
use mint_sage_passability::{DynamicView, GROUND};
use mint_sim::{Component, ComponentId, Entity, Frame, StateHasher, Tick, World};

use crate::obstacles::{counts_of, rubble_of, ObstacleLayer};
use crate::unit::{Body, Spawn};

#[allow(clippy::cast_precision_loss)]
const RETAIL_FRAME_SECONDS: f32 = 1.0 / crate::game_data::RETAIL_LOGIC_HZ as f32;

const SETTLE_TURN: f32 = 0.174_532_92;

const OATH_FULLFILLED: &str = "OATH_FULLFILLED";

const AOD_JUST_BUILT: &str = "JUST_BUILT";

pub(crate) const JOIN_FADE_DURATION: mint_sim::LogicFrames =
    mint_sim::LogicFrames::from_milliseconds(5_000);

const CATCH_UP_STEPS: f32 = 4.0;

const CATCH_UP_FACTOR: f32 = 1.5;

pub(crate) fn decision_stagger(dt: f32) -> Stagger {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let period = (RETAIL_FRAME_SECONDS / dt).round().max(1.0) as u32;
    Stagger::every(period)
}

const HORSE_SLOW_FRACTION: f32 = 0.25;

const HORSE_GAIT_FRACTION: f32 = 0.66;

const HORSE_FACING_GAP: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FormationMode {
    #[default]
    On,
    Off,
}

impl FormationMode {
    pub(crate) const ALL: [Self; 2] = [Self::On, Self::Off];

    #[must_use]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::On => "formations on (horde paths, members hold slots)",
            Self::Off => "formations off (every member paths for itself)",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Horde {
    pub(crate) formation: u16,
    pub(crate) slots: Vec<HordeSlot>,
    pub(crate) members: Vec<Entity>,
    pub(crate) free: Vec<u16>,
    pub(crate) banner_slot: Option<u16>,
    pub(crate) dirty: bool,
    pub(crate) centre: Vec2,
    pub(crate) climb_wall: Option<Entity>,
    pub(crate) climb_quiet: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HordeSlot {
    pub(crate) offset: Vec2,
    pub(crate) rank: u32,
    pub(crate) unit_type: Option<u16>,
    pub(crate) index_in_rank: u32,
    pub(crate) leader: Option<mint_sage_formation::LeaderRef>,
}

impl Horde {
    pub(crate) fn offset(&self, slot: u16) -> Option<Vec2> {
        self.slots.get(usize::from(slot)).map(|slot| slot.offset)
    }

    pub(crate) fn offsets(&self) -> impl Iterator<Item = Vec2> + '_ {
        self.slots.iter().map(|slot| slot.offset)
    }
}

impl Component for Horde {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u16(self.formation);
        hasher.write_usize(self.slots.len());
        for slot in &self.slots {
            hasher.write_f32(slot.offset.x);
            hasher.write_f32(slot.offset.y);
            hasher.write_u32(slot.rank);
            hasher.write_u16(slot.unit_type.unwrap_or(u16::MAX));
            hasher.write_u32(slot.index_in_rank);
            match slot.leader {
                Some(leader) => {
                    hasher.write_u8(1);
                    hasher.write_u32(leader.rank);
                    hasher.write_u32(leader.index);
                }
                None => hasher.write_u8(0),
            }
        }
        hasher.write_usize(self.members.len());
        for member in &self.members {
            member.hash_into(hasher);
        }
        hasher.write_usize(self.free.len());
        for slot in &self.free {
            hasher.write_u16(*slot);
        }
        match self.banner_slot {
            Some(slot) => {
                hasher.write_u8(1);
                hasher.write_u16(slot);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_bool(self.dirty);
        hasher.write_f32(self.centre.x);
        hasher.write_f32(self.centre.y);
        match self.climb_wall {
            Some(wall) => {
                hasher.write_u8(1);
                wall.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_u8(self.climb_quiet);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Member {
    pub(crate) horde: Entity,
    pub(crate) slot: Option<u16>,
    pub(crate) parked: bool,
}

impl Component for Member {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.horde.hash_into(hasher);
        match self.slot {
            Some(slot) => {
                hasher.write_u8(1);
                hasher.write_u16(slot);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_bool(self.parked);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HordeMemberMark;

impl Component for HordeMemberMark {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct JoinFade {
    start: Frame,
    end: Frame,
}

impl JoinFade {
    pub(crate) fn starting(now: Frame, duration: mint_sim::LogicFrames) -> Self {
        Self {
            start: now,
            end: now.checked_add(duration).unwrap_or(Frame::FOREVER),
        }
    }

    pub(crate) fn opacity(&self, now: Frame) -> f32 {
        if now <= self.start {
            return 0.0;
        }
        if now >= self.end {
            return 1.0;
        }
        let span = self.end.get().saturating_sub(self.start.get()).max(1);
        let elapsed = now.get().saturating_sub(self.start.get());
        #[allow(clippy::cast_precision_loss)]
        let fraction = elapsed as f32 / span as f32;
        fraction
    }
}

impl Component for JoinFade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.start.hash_into(hasher);
        self.end.hash_into(hasher);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HordeSpawn {
    pub(crate) formation: u16,
    pub(crate) origin: Vec2,
    pub(crate) facing: Facing,
    pub(crate) member: Spawn,
    pub(crate) count: u32,
    pub(crate) vision: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HordeIds {
    pub(crate) horde: mint_sim::ComponentId<Horde>,
    pub(crate) member: mint_sim::ComponentId<Member>,
    pub(crate) member_mark: mint_sim::ComponentId<HordeMemberMark>,
    pub(crate) body: mint_sim::ComponentId<Body>,
    pub(crate) path: mint_sim::ComponentId<crate::unit::Path>,
    pub(crate) doomed: mint_sim::ComponentId<crate::combat::Doomed>,
    pub(crate) health: mint_sim::ComponentId<crate::combat::Health>,
    pub(crate) shape: mint_sim::ComponentId<crate::unit::Shape>,
    pub(crate) bonuses: mint_sim::ComponentId<crate::experience::Bonuses>,
    pub(crate) kind: mint_sim::ComponentId<crate::unit::Kind>,
    pub(crate) locomotion: mint_sim::ComponentId<crate::unit::Locomotion>,
    pub(crate) motive: mint_sim::ComponentId<crate::unit::Motive>,
    pub(crate) owner: mint_sim::ComponentId<crate::unit::Owner>,
    pub(crate) portals: mint_sim::ComponentId<crate::portals::PortalLayer>,
    pub(crate) wall_layer: mint_sim::ComponentId<crate::wall_layer::WallLayer>,
    pub(crate) horde_melee: mint_sim::ComponentId<crate::melee::HordeMelee>,
    pub(crate) member_melee: mint_sim::ComponentId<crate::melee::MemberMelee>,
    pub(crate) member_attack: mint_sim::ComponentId<crate::melee::MemberAttack>,
    pub(crate) goal_cells: mint_sim::ComponentId<crate::goal_cells::GoalCells>,
    pub(crate) horde_trail: mint_sim::ComponentId<crate::horde_trail::HordeTrail>,
    pub(crate) glided: mint_sim::ComponentId<crate::horde_trail::Glided>,
    pub(crate) human_barred: mint_sim::ComponentId<crate::rules::border::HumanBarred>,
    pub(crate) layer: mint_sim::ComponentId<crate::wall_layer::Layer>,
    pub(crate) aod_crusher: mint_sim::ComponentId<crate::rules::AodCrusher>,
    pub(crate) using_ability: mint_sim::ComponentId<crate::rules::UsingAbility>,
    pub(crate) join_fade: mint_sim::ComponentId<JoinFade>,
    pub(crate) elevation: mint_sim::ComponentId<crate::unit::Elevation>,
    pub(crate) scaleable_walls: mint_sim::ComponentId<crate::obstacles::ScaleableWalls>,
    pub(crate) combat_facts: mint_sim::ComponentId<crate::combat_facts::CombatFacts>,
}

fn hordes(world: &World, ids: HordeIds) -> Vec<Entity> {
    world
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default()
}

pub(crate) fn member_slot_pose(world: &World, ids: HordeIds, member: Entity) -> Option<Vec2> {
    let entry = world.get(ids.member, member)?;
    let horde = world.get(ids.horde, entry.horde)?;
    let offset = horde.offset(entry.slot?)?;
    let body = world.get(ids.body, entry.horde)?;
    Some(FormationBasis::of(body.0.facing).place(body.0.position, offset))
}

pub(crate) fn horde_slot_pose(
    world: &World,
    ids: HordeIds,
    horde: Entity,
    index: u16,
) -> Option<Vec2> {
    let def = world.get(ids.horde, horde)?;
    let body = world.get(ids.body, horde)?;
    let offset = def.offset(index)?;
    Some(FormationBasis::of(body.0.facing).place(body.0.position, offset))
}

pub(crate) fn slot_poses(horde: &Horde, body: &MoverState) -> Vec<Vec2> {
    let basis = FormationBasis::of(body.facing);
    horde
        .offsets()
        .map(|offset| basis.place(body.position, offset))
        .collect()
}

pub(crate) fn slot_pose_from(
    horde: &Horde,
    position: Vec2,
    basis: FormationBasis,
    slot: usize,
) -> Option<Vec2> {
    let offset = horde.slots.get(slot)?.offset;
    Some(basis.place(position, offset))
}

pub(crate) struct Straggler {
    pub(crate) member: Entity,
    pub(crate) rescue_target: Vec2,
    pub(crate) place_on_failure: bool,
    pub(crate) speed: f32,
}

fn gated_view<'a>(
    tick: &'a Tick<'_>,
    grid: &'a CostGrid,
    obstacles: ComponentId<ObstacleLayer>,
    portals: mint_sim::ComponentId<crate::portals::PortalLayer>,
    wall_layer: mint_sim::ComponentId<crate::wall_layer::WallLayer>,
    human_barred: mint_sim::ComponentId<crate::rules::border::HumanBarred>,
) -> mint_sage_passability::GatedView<'a> {
    mint_sage_passability::GatedView {
        ground: DynamicView {
            grid,
            counts: counts_of(tick.state(), obstacles),
            rubble: rubble_of(tick.state(), obstacles),
            wall_rim: crate::wall_layer::rim_of(tick.state(), wall_layer),
            tags: crate::wall_layer::tags_of(tick.state(), wall_layer),
            connect: crate::wall_layer::connect_of(tick.state(), wall_layer),
            released: crate::wall_layer::released_of(tick.state(), wall_layer),
            closed: crate::obstacles::closed_of(tick.state(), obstacles),
            obstacle_pinched: crate::obstacles::pinched_of(tick.state(), obstacles),
            ramp_side: crate::wall_layer::side_of(tick.state(), wall_layer),
            scaleable: crate::obstacles::scaleable_of(tick.state(), obstacles),
            barred: crate::rules::border::barred_of(tick.state(), human_barred),
        },
        portals: crate::portals::masks_of(tick.state(), portals),
        tags: &[],
    }
}

fn mover_class(
    tick: &Tick<'_>,
    ids: HordeIds,
    entity: Entity,
    kinds: &crate::templates::KindTable,
) -> MoverClass {
    let terrain = tick
        .state()
        .get(ids.kind, entity)
        .copied()
        .and_then(|kind| kinds.get(kind))
        .and_then(|entry| entry.mover)
        .unwrap_or(GROUND);
    let asker = tick
        .state()
        .get(ids.owner, entity)
        .map_or(mint_movement::Asker::NOBODY, |owner| {
            mint_movement::Asker::new(owner.0.get())
        });
    terrain.asking(mint_sage_passability::PORTAL, asker)
}

fn step_valid(
    view: &mint_sage_passability::GatedView<'_>,
    geometry: CellGeometry,
    mover: MoverClass,
    (from, to, goal): (Vec2, Vec2, Vec2),
) -> bool {
    let cell = |at: Vec2| geometry.cell_at(at, 0, view.extent());
    let open = |at: Vec2| cell(at).is_some_and(|c| view.is_passable(c, mover));
    let same_cell = cell(to).is_some() && cell(to) == cell(goal);
    open(to) || !open(from) || same_cell
}

const SLOT_CLAMP_HEIGHT_SPREAD: f32 = 10.0;

const SLOT_CLAMP_CANDIDATES: u32 = 15;

fn reachability_for<'a>(
    ground: &'a Reachability,
    extra: &'a [(MoverClass, Reachability)],
    mover: MoverClass,
) -> &'a Reachability {
    let terrain = mover.terrain();
    if terrain == ground.mover() {
        return ground;
    }
    extra
        .iter()
        .find(|(class, _)| *class == terrain)
        .map_or(ground, |(_, oracle)| oracle)
}

#[allow(clippy::too_many_arguments)]
fn clamp_member_slot(
    view: &mint_sage_passability::GatedView<'_>,
    geometry: CellGeometry,
    heights: &HeightField,
    reachability: &Reachability,
    mover: MoverClass,
    footprint: crate::goal_cells::Footprint,
    horde_cell: Option<Cell>,
    horde_pos: Vec2,
    slot: Vec2,
) -> Vec2 {
    let Some(horde_cell) = horde_cell else {
        return slot;
    };
    let fits = |at: Vec2| {
        slot_footprint_fits(
            view,
            geometry,
            heights,
            reachability,
            mover,
            footprint,
            horde_cell,
            at,
        )
    };
    if fits(slot) {
        return slot;
    }
    for step in 1..=SLOT_CLAMP_CANDIDATES {
        #[allow(clippy::cast_precision_loss)]
        let t = f32::from(u16::try_from(step).unwrap_or(u16::MAX)) / 16.0;
        let candidate = Vec2::new(
            slot.x + (horde_pos.x - slot.x) * t,
            slot.y + (horde_pos.y - slot.y) * t,
        );
        if fits(candidate) {
            return candidate;
        }
    }
    geometry.centre_of(horde_cell)
}

fn slot_footprint_fits(
    view: &mint_sage_passability::GatedView<'_>,
    geometry: CellGeometry,
    heights: &HeightField,
    reachability: &Reachability,
    mover: MoverClass,
    footprint: crate::goal_cells::Footprint,
    horde_cell: Cell,
    at: Vec2,
) -> bool {
    let anchor = crate::goal_cells::anchor(geometry, at, footprint);
    let Some(rect) = crate::goal_cells::rect(view.extent(), anchor, footprint) else {
        return false;
    };
    let (x0, y0, x1, y1) = rect.span();
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for y in y0..y1 {
        for x in x0..x1 {
            let (Ok(cx), Ok(cy)) = (u16::try_from(x), u16::try_from(y)) else {
                return false;
            };
            let cell = Cell::new(cx, cy);
            if !view.is_passable(cell, mover) {
                return false;
            }
            let Some(height) = heights.height_at_point(geometry.centre_of(cell)) else {
                return false;
            };
            low = low.min(height);
            high = high.max(height);
        }
    }
    if high - low > SLOT_CLAMP_HEIGHT_SPREAD {
        return false;
    }
    let Some(candidate_cell) = geometry.cell_at(at, 0, view.extent()) else {
        return false;
    };
    reachability.same_component(candidate_cell, horde_cell)
}

fn holds_condition(
    tick: &Tick<'_>,
    state: Option<ComponentId<mint_sage_modules::effects::UpgradeState>>,
    entity: Entity,
    name: &str,
) -> bool {
    state.is_some_and(|state| {
        tick.state().get(state, entity).is_some_and(|held| {
            held.model_conditions
                .iter()
                .any(|condition| condition.eq_ignore_ascii_case(name))
        })
    })
}

pub(crate) struct PassInputs<'a> {
    pub(crate) held_elsewhere: &'a BTreeSet<Entity>,
    pub(crate) touched: &'a BTreeSet<Entity>,
    pub(crate) kinds: &'a crate::templates::KindTable,
    pub(crate) formations: &'a [FormationTable],
    pub(crate) extent: mint_movement::GridExtent,
    pub(crate) attacking: &'a BTreeSet<Entity>,
    pub(crate) heights: &'a HeightField,
    pub(crate) reachability: &'a Reachability,
    pub(crate) reachability_extra: &'a [(MoverClass, Reachability)],
    pub(crate) model_conditions: Option<ComponentId<mint_sage_modules::effects::UpgradeState>>,
    pub(crate) restriction: crate::rules::border::Restriction<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Act {
    Step { goal: Vec2, step: f32 },
    TurnStep(Facing),
    Snap(Facing),
    Stand,
    Settle(Option<Vec2>),
    Glide(crate::horde_trail::TrailPlace),
}

#[derive(Debug, Clone, Copy)]
struct HordeMotion {
    facing: Facing,
    moving: bool,
    slow_movement: bool,
    horse: bool,
    horse_slow: bool,
}

fn hub(
    state: MoverState,
    slot: Vec2,
    horde: HordeMotion,
    limits: &mint_movement::MotionLimits,
    dt: f32,
) -> Act {
    let offset = slot.minus(state.position);
    if horde.moving && horde.slow_movement && horde.facing.to_vector().dot(offset) < 0.0 {
        return Act::Stand;
    }
    let gap = offset.length();
    if gap >= slot_arrive(limits, horde.moving, dt) || gap.is_nan() {
        let off_heading = state.facing.delta_to(horde.facing).abs();
        if horde.horse_slow && off_heading > 0.5 * limits.turn_rate * RETAIL_FRAME_SECONDS {
            return Act::TurnStep(horde.facing);
        }
        if horde.horse
            && state.speed == 0.0
            && gap < limits.acceleration * RETAIL_FRAME_SECONDS * RETAIL_FRAME_SECONDS
        {
            return Act::Settle(None);
        }
        let speed = if horde.horse {
            (gap / RETAIL_FRAME_SECONDS).min(limits.max_speed)
        } else {
            limits.max_speed
        };
        let catching_up =
            horde.moving && gap > CATCH_UP_STEPS * limits.max_speed * RETAIL_FRAME_SECONDS;
        let factor = if catching_up { CATCH_UP_FACTOR } else { 1.0 };
        return Act::Step {
            goal: slot,
            step: speed * factor * dt,
        };
    }
    if state.facing.delta_to(horde.facing).abs() > SETTLE_TURN {
        return Act::Snap(horde.facing);
    }
    if horde.moving {
        Act::Stand
    } else {
        Act::Settle(Some(slot))
    }
}

fn slot_arrive(limits: &mint_movement::MotionLimits, slot_moving: bool, dt: f32) -> f32 {
    if slot_moving {
        arrive(limits, dt)
    } else {
        (limits.acceleration * RETAIL_FRAME_SECONDS).min(0.2 * limits.max_speed)
            * RETAIL_FRAME_SECONDS
    }
}

pub(crate) fn arrive(limits: &mint_movement::MotionLimits, dt: f32) -> f32 {
    (limits.acceleration * RETAIL_FRAME_SECONDS).min(0.2 * limits.max_speed) * dt
}

fn claim_goal(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    inputs: &PassInputs<'_>,
    geometry: CellGeometry,
    member: Entity,
    at: Vec2,
) {
    let footprint =
        crate::goal_cells::footprint_of(tick.state(), ids, inputs.kinds, member, geometry);
    let cell = crate::goal_cells::anchor(geometry, at, footprint);
    let Some(rect) = crate::goal_cells::rect(inputs.extent, cell, footprint) else {
        return;
    };
    if let Some(layer) = crate::goal_cells::layer_mut(tick, ids.goal_cells) {
        layer.claim(member, rect);
    }
}

fn leave_goal(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    inputs: &PassInputs<'_>,
    geometry: CellGeometry,
    member: Entity,
    goal: Vec2,
) {
    let Some(held) =
        crate::goal_cells::layer(tick.state(), ids.goal_cells).and_then(|layer| layer.held(member))
    else {
        return;
    };
    let footprint =
        crate::goal_cells::footprint_of(tick.state(), ids, inputs.kinds, member, geometry);
    let cell = crate::goal_cells::anchor(geometry, goal, footprint);
    if crate::goal_cells::rect(inputs.extent, cell, footprint) == Some(held) {
        return;
    }
    if let Some(layer) = crate::goal_cells::layer_mut(tick, ids.goal_cells) {
        layer.release(member);
    }
}

fn trail_lines(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    horde_entity: Entity,
    body: &MoverState,
    limits: Option<mint_movement::MotionLimits>,
    dt: f32,
    horde_oath: bool,
) -> mint_sim::Result<Option<(crate::horde_trail::HordeTrail, Vec<Vec2>)>> {
    let Some(mut trail) = tick.state().get(ids.horde_trail, horde_entity).cloned() else {
        return Ok(None);
    };
    if decision_stagger(dt).is_due(MoverId(horde_entity.index()), tick.frame().get()) {
        let spacing = limits.map_or(0.0, |limits| limits.acceleration * RETAIL_FRAME_SECONDS);
        trail.lay(body.position, body.facing, spacing);
        trail.advance_oath(horde_oath, limits.map_or(0.0, |limits| limits.max_speed));
    }
    let lines = trail.place_lines(body.position, body.facing, dt / RETAIL_FRAME_SECONDS);
    tick.state_mut()
        .insert(ids.horde_trail, horde_entity, trail.clone())?;
    Ok(Some((trail, lines)))
}

fn member_limits(
    tick: &Tick<'_>,
    ids: HordeIds,
    member: Entity,
    kinds: &crate::templates::KindTable,
) -> mint_movement::MotionLimits {
    let base = crate::unit::current_limits(
        tick.state(),
        kinds,
        (ids.kind, ids.locomotion, ids.health),
        member,
        None,
        1.0,
    )
    .unwrap_or(crate::unit::PLACEHOLDER_LIMITS);
    crate::experience::with_speed(base, tick.state().get(ids.bonuses, member))
}

fn large_unit_lift(
    sway: &mint_sage_formation::AodSway,
    touch: crate::rules::LargeUnitTouch,
    facing: Facing,
    position: Vec2,
    now: mint_sim::Frame,
) -> f32 {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let timeout =
        mint_sim::LogicFrames::from_milliseconds(sway.large_unit_timeout_ms.max(0.0) as u32);
    if timeout.is_zero() || sway.large_unit_tail_off <= 0.0 {
        return 0.0;
    }
    let elapsed = now.get().saturating_sub(touch.since.get());
    if elapsed >= u64::from(timeout.get()) {
        return 0.0;
    }
    let height = (sway.large_unit_height_factor * touch.height)
        .max(sway.large_unit_min_height)
        .min(sway.large_unit_max_height);
    if height <= 0.0 {
        return 0.0;
    }
    let limit = height * sway.large_unit_tail_off;
    if limit <= 0.0 {
        return 0.0;
    }
    let local = FormationBasis::of(facing).project(touch.position, position);
    let r = local.x.abs() + (local.y.abs() - 17.0).max(0.0);
    if r >= limit {
        return 0.0;
    }
    let t = (r / limit).clamp(0.0, 1.0);
    let g = t * t.sqrt() + t * t * (1.0 - t);
    #[allow(clippy::cast_precision_loss)]
    let half = timeout.get() as f32 * 0.5;
    #[allow(clippy::cast_precision_loss)]
    let elapsed_f = elapsed as f32;
    let fade = if elapsed_f < half {
        1.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let total = timeout.get() as f32;
        (2.0 * (1.0 - elapsed_f / total)).clamp(0.0, 1.0)
    };
    (1.0 - g) * height * fade
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn advance_members(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    obstacles: ComponentId<ObstacleLayer>,
    grid: &CostGrid,
    geometry: CellGeometry,
    dt: f32,
    inputs: &PassInputs<'_>,
) -> mint_sim::Result<(
    Vec<Straggler>,
    Vec<Entity>,
    Vec<(Entity, mint_sage_modules::effects::Effect)>,
)> {
    let mut stragglers = Vec::new();
    let mut porcupine_arrivals = Vec::new();
    let mut oath_effects: Vec<(Entity, mint_sage_modules::effects::Effect)> = Vec::new();
    for horde_entity in hordes(tick.state(), ids) {
        let moving = tick.state().contains(ids.path, horde_entity);
        let Some(horde) = tick.state().get(ids.horde, horde_entity) else {
            continue;
        };
        if !moving
            && !horde.dirty
            && !inputs.touched.contains(&horde_entity)
            && !tick.state().contains(ids.horde_melee, horde_entity)
            && !tick.state().contains(ids.horde_trail, horde_entity)
        {
            continue;
        }
        crate::diag::diag!(
            "members: frame {} horde {} formation pass (moving {moving}, dirty {}, touched {}, \
             fighting {})",
            tick.frame().get(),
            horde_entity.index(),
            horde.dirty,
            inputs.touched.contains(&horde_entity),
            tick.state().contains(ids.horde_melee, horde_entity)
        );
        let horde = horde.clone();
        let Some(body) = tick.state().get(ids.body, horde_entity).copied() else {
            continue;
        };
        let settings = inputs
            .formations
            .get(usize::from(horde.formation))
            .map(FormationTable::settings);
        let horse = settings.is_some_and(|settings| settings.horse);
        let porcupine = settings.is_some_and(|settings| settings.porcupine);
        let oath_sway = settings.and_then(|settings| settings.aod);
        let horde_oath = oath_sway.is_some()
            && holds_condition(tick, inputs.model_conditions, horde_entity, OATH_FULLFILLED);
        let in_melee = tick
            .state()
            .get(ids.horde_melee, horde_entity)
            .is_some_and(|melee| melee.victim.is_some());
        let horde_limits = crate::unit::current_limits(
            tick.state(),
            inputs.kinds,
            (ids.kind, ids.locomotion, ids.health),
            horde_entity,
            None,
            1.0,
        );
        let top = horde_limits.map_or(0.0, |limits| limits.max_speed);
        let horse_slow = !in_melee && horse && body.0.speed <= HORSE_SLOW_FRACTION * top;
        let horde_pulling_up = horse
            && !in_melee
            && body.0.speed < HORSE_GAIT_FRACTION * top
            && (!moving
                || tick
                    .state()
                    .get(ids.motive, horde_entity)
                    .is_some_and(|motive| motive.0.contains(MotionPhase::DECELERATING)));
        let motion = HordeMotion {
            facing: body.0.facing,
            moving,
            slow_movement: settings.is_some_and(|settings| settings.use_slow_horde_movement),
            horse,
            horse_slow,
        };
        let poses = slot_poses(&horde, &body.0);
        let horde_cell = geometry.cell_at(body.0.position, 0, inputs.extent);
        #[allow(clippy::cast_precision_loss)]
        let frames_left = decision_stagger(dt)
            .frames_until_due(MoverId(horde_entity.index()), tick.frame().get())
            as f32;
        let fixed_goal = in_melee || !moving;
        let trail = trail_lines(
            tick,
            ids,
            horde_entity,
            &body.0,
            horde_limits,
            dt,
            horde_oath,
        )?;
        let pace = if top > 0.0 { body.0.speed / top } else { 0.0 };
        #[allow(clippy::cast_precision_loss)]
        let retail_frame = tick.frame().get() as f32 * dt / RETAIL_FRAME_SECONDS;
        let mut unsettled = false;
        for member in horde.members.iter().copied() {
            let stale = tick.state().get(ids.path, member).is_some_and(|path| {
                path.kind == crate::unit::PathKind::Formation
                    && tick
                        .state()
                        .get(ids.member_attack, member)
                        .and_then(|attack| attack.spot)
                        .is_some_and(|spot| spot.minus(path.goal).length() > 1.0)
            });
            if stale {
                tick.state_mut().remove(ids.path, member);
            }
            if tick.state().contains(ids.path, member) || inputs.held_elsewhere.contains(&member) {
                unsettled = true;
                continue;
            }
            let Some(entry) = tick.state().get(ids.member, member).copied() else {
                continue;
            };
            let Some(state) = tick.state().get(ids.body, member).map(|body| body.0) else {
                continue;
            };
            let unparked = Member {
                parked: entry.parked && moving,
                ..entry
            };
            if unparked != entry {
                tick.state_mut().insert(ids.member, member, unparked)?;
            }
            let limits = member_limits(tick, ids, member, inputs.kinds);
            let mover = inputs.restriction.apply(
                mover_class(tick, ids, member, inputs.kinds),
                tick.state().get(ids.owner, member).map(|owner| owner.0),
            );
            let attack = tick
                .state()
                .get(ids.member_attack, member)
                .copied()
                .filter(|attack| {
                    tick.state()
                        .get(ids.health, attack.victim)
                        .is_some_and(|health| !health.0.is_dead())
                });
            let fighting = attack.is_some();
            let engage = attack
                .and_then(|attack| attack.spot)
                .filter(|spot| spot.minus(state.position).length() >= arrive(&limits, dt));
            let record = if in_melee {
                tick.state().get(ids.member_melee, member).copied()
            } else {
                None
            };
            if let Some(spot) = engage {
                stragglers.push(Straggler {
                    member,
                    rescue_target: spot,
                    place_on_failure: false,
                    speed: state.speed,
                });
                unsettled = true;
                continue;
            }
            let slot = entry
                .slot
                .and_then(|slot| poses.get(usize::from(slot)).copied());
            let facing = match slot {
                Some(slot) if porcupine => porcupine_facing(body.0.position, slot, motion.facing),
                _ => motion.facing,
            };
            let mut skip_move = false;
            let member_oath = if oath_sway.is_some() {
                let member_oath =
                    holds_condition(tick, inputs.model_conditions, member, OATH_FULLFILLED);
                if horde_oath && !member_oath {
                    oath_effects.push((
                        member,
                        mint_sage_modules::effects::Effect::ModelConditions {
                            add: mint_sage_modules::effects::names(
                                vec![OATH_FULLFILLED.to_owned()],
                            ),
                            remove: mint_sage_modules::effects::names(Vec::new()),
                        },
                    ));
                } else if !horde_oath && member_oath {
                    oath_effects.push((
                        member,
                        mint_sage_modules::effects::Effect::ModelConditions {
                            add: mint_sage_modules::effects::names(Vec::new()),
                            remove: mint_sage_modules::effects::names(vec![
                                OATH_FULLFILLED.to_owned()
                            ]),
                        },
                    ));
                }
                if holds_condition(tick, inputs.model_conditions, member, AOD_JUST_BUILT) {
                    oath_effects.push((
                        member,
                        mint_sage_modules::effects::Effect::ModelConditions {
                            add: mint_sage_modules::effects::names(Vec::new()),
                            remove: mint_sage_modules::effects::names(vec![
                                AOD_JUST_BUILT.to_owned()
                            ]),
                        },
                    ));
                    skip_move = true;
                }
                if tick.state().contains(ids.using_ability, member) {
                    skip_move = true;
                }
                member_oath
            } else {
                false
            };
            let (mut act, rescue) = if fighting || (!moving && inputs.attacking.contains(&member)) {
                (Act::Stand, false)
            } else if in_melee {
                match record {
                    Some(record)
                        if record.goal.minus(state.position).length() >= arrive(&limits, dt) =>
                    {
                        (
                            Act::Step {
                                goal: record.goal,
                                step: limits.max_speed * dt,
                            },
                            true,
                        )
                    }
                    Some(_) => (Act::Snap(facing), false),
                    None => (Act::Stand, false),
                }
            } else if let Some((trail, lines)) = &trail {
                let place = entry.slot.and_then(|index| {
                    horde.slots.get(usize::from(index)).and_then(|slot| {
                        trail.place(lines, slot, usize::from(index), retail_frame, pace)
                    })
                });
                let Some(mut place) = place else {
                    continue;
                };
                if let Some(sway) = oath_sway {
                    if member_oath {
                        if let Some(index) = entry.slot {
                            let direction = horde.slots.get(usize::from(index)).and_then(|slot| {
                                FormationBasis::of(body.0.facing)
                                    .place(Vec2::ZERO, slot.offset)
                                    .normalised()
                            });
                            let (xy, z) = trail.oath_scatter(direction, usize::from(index));
                            place.position = place.position.plus(xy);
                            place.height += z;
                            if let Some(heading) = direction.and_then(Facing::from_vector) {
                                place.facing =
                                    place.facing.turned_towards(heading, limits.turn_rate * dt);
                            }
                        }
                    }
                    if let Some(touch) = tick
                        .state()
                        .get(ids.aod_crusher, member)
                        .and_then(|crusher| crusher.large_unit)
                    {
                        place.height += large_unit_lift(
                            &sway,
                            touch,
                            body.0.facing,
                            place.position,
                            tick.frame(),
                        );
                    }
                }
                (Act::Glide(place), false)
            } else {
                let Some(slot) = slot else {
                    continue;
                };
                let footprint = crate::goal_cells::footprint_of(
                    tick.state(),
                    ids,
                    inputs.kinds,
                    member,
                    geometry,
                );
                let view = gated_view(
                    tick,
                    grid,
                    obstacles,
                    ids.portals,
                    ids.wall_layer,
                    ids.human_barred,
                );
                let reachability =
                    reachability_for(inputs.reachability, inputs.reachability_extra, mover);
                let slot = clamp_member_slot(
                    &view,
                    geometry,
                    inputs.heights,
                    reachability,
                    mover,
                    footprint,
                    horde_cell,
                    body.0.position,
                    slot,
                );
                (
                    hub(state, slot, HordeMotion { facing, ..motion }, &limits, dt),
                    true,
                )
            };
            if skip_move {
                act = Act::Stand;
            }
            if matches!(act, Act::Glide(_)) {
                if !tick.state().contains(ids.glided, member) {
                    tick.state_mut()
                        .insert(ids.glided, member, crate::horde_trail::Glided)?;
                }
            } else {
                tick.state_mut().remove(ids.glided, member);
            }
            if porcupine && matches!(act, Act::Settle(_)) {
                porcupine_arrivals.push(member);
            }
            match act {
                Act::Settle(Some(slot)) => claim_goal(tick, ids, inputs, geometry, member, slot),
                Act::Step { goal, .. } => leave_goal(tick, ids, inputs, geometry, member, goal),
                Act::Glide(place) => {
                    leave_goal(tick, ids, inputs, geometry, member, place.position);
                }
                Act::Snap(_) | Act::TurnStep(_) | Act::Stand | Act::Settle(None) => {}
            }
            if !matches!(act, Act::Settle(_)) {
                unsettled = true;
            }
            let mut next = state;
            next.speed = 0.0;
            next.waypoint = 0;
            let mut phase = MotionPhase::NONE;
            match act {
                Act::Step { goal, step } => {
                    let offset = goal.minus(state.position);
                    let gap = offset.length();
                    let step = if fixed_goal {
                        step.min(gap / frames_left)
                    } else {
                        step
                    };
                    let to = if gap > step {
                        state.position.plus(offset.scale(step / gap))
                    } else {
                        goal
                    };
                    let was_pulling_up = tick
                        .state()
                        .get(ids.motive, member)
                        .is_some_and(|motive| motive.0.contains(MotionPhase::DECELERATING));
                    if horde_pulling_up && (gap < step || was_pulling_up) {
                        phase = MotionPhase::DECELERATING;
                    }
                    let view = gated_view(
                        tick,
                        grid,
                        obstacles,
                        ids.portals,
                        ids.wall_layer,
                        ids.human_barred,
                    );
                    let passable = step_valid(&view, geometry, mover, (state.position, to, goal));
                    if passable {
                        next.position = to;
                        if let Some(travel) = Facing::from_vector(offset) {
                            let share = if horse {
                                (gap / HORSE_FACING_GAP).min(1.0)
                            } else {
                                1.0
                            };
                            let turn = state.facing.delta_to(travel).abs() * share / frames_left;
                            next.facing = state.facing.turned_towards(travel, turn);
                        }
                        next.speed = to.minus(state.position).length().min(step) / dt;
                    } else if rescue {
                        stragglers.push(Straggler {
                            member,
                            rescue_target: goal,
                            place_on_failure: true,
                            speed: state.speed,
                        });
                    }
                }
                Act::TurnStep(towards) => {
                    let delta = state.facing.delta_to(towards);
                    next.facing = state.facing.turned_towards(towards, limits.turn_rate * dt);
                    phase = if delta > 0.0 {
                        MotionPhase::TURNING_LEFT
                    } else {
                        MotionPhase::TURNING_RIGHT
                    };
                }
                Act::Snap(towards) => {
                    let delta = state.facing.delta_to(towards);
                    let turn = delta.abs() / frames_left;
                    next.facing = state.facing.turned_towards(towards, turn);
                    if delta.abs() > 0.5 * limits.turn_rate * RETAIL_FRAME_SECONDS {
                        phase = if delta > 0.0 {
                            MotionPhase::TURNING_LEFT
                        } else {
                            MotionPhase::TURNING_RIGHT
                        };
                    }
                }
                Act::Glide(place) => {
                    let body_layer = tick
                        .state()
                        .get(ids.layer, horde_entity)
                        .copied()
                        .unwrap_or(crate::wall_layer::Layer::GROUND);
                    let step = crate::horde_trail::MEMBER_STEP_FACTOR * limits.max_speed * dt;
                    let extent = grid.extent();
                    let layer_height = |point: Vec2| {
                        let ground = inputs.heights.height_at_point(point);
                        crate::wall_layer::layer_of(tick.state(), ids.wall_layer)
                            .and_then(|wall| {
                                wall.wall_height(geometry, extent, point, body_layer, ground)
                            })
                            .or(ground)
                    };
                    let from_z = tick
                        .state()
                        .get(ids.elevation, member)
                        .map(|elevation| elevation.0)
                        .or_else(|| layer_height(state.position));
                    let to_z = layer_height(place.position).map(|z| z + place.height);
                    let rise = match (from_z, to_z) {
                        (Some(from), Some(to)) => to - from,
                        _ => 0.0,
                    };
                    let offset = place.position.minus(state.position);
                    let flat = offset.length();
                    let gap = (flat * flat + rise * rise).sqrt();
                    let fraction = if gap > step { step / gap } else { 1.0 };
                    next.position = if gap > step {
                        state.position.plus(offset.scale(fraction))
                    } else {
                        place.position
                    };
                    if let Some(from) = from_z.filter(|_| to_z.is_some()) {
                        let z = from + rise * fraction;
                        match tick.state_mut().get_mut(ids.elevation, member) {
                            Some(elevation) => elevation.0 = z,
                            None => {
                                tick.state_mut().insert(
                                    ids.elevation,
                                    member,
                                    crate::unit::Elevation::new(z),
                                )?;
                            }
                        }
                    }
                    next.facing = place.facing;
                    if moving {
                        next.speed = gap.min(step) / dt;
                    }
                }
                Act::Stand | Act::Settle(None) => {}
                Act::Settle(Some(slot)) => next.position = slot,
            }
            if next != state {
                tick.state_mut().insert(ids.body, member, Body(next))?;
            }
            if tick.state().get(ids.motive, member).map(|motive| motive.0) != Some(phase) {
                tick.state_mut()
                    .insert(ids.motive, member, crate::unit::Motive(phase))?;
            }
        }
        let dirty = moving || unsettled;
        if dirty != horde.dirty {
            let mut updated = horde;
            updated.dirty = dirty;
            tick.state_mut().insert(ids.horde, horde_entity, updated)?;
        }
    }
    Ok((stragglers, porcupine_arrivals, oath_effects))
}

const CLIMBING: &str = "CLIMBING";
const RAPPELLING: &str = "RAPPELLING";
const SCALING_WALL: &str = "SCALING_WALL";
const SCALING_WALL_HORDE: &str = "SCALING_WALL_HORDE";

const CLIMB_RATE_SHARE: f32 = 0.6;

const CLIMB_GROUNDED_TOLERANCE: f32 = 0.01;

const CLIMB_CLEAR_SLOT_DISTANCE: f32 = 0.5;

const CLIMB_CLEAR_QUIET_CHECKS: u8 = 4;

fn wall_contains(point: Vec2, wall: &mint_sage_passability::WallGeometry) -> bool {
    let (sin, cos) = mint_math::f32::sin_cos(wall.angle);
    let dx = point.x - wall.centre.x;
    let dy = point.y - wall.centre.y;
    let local_x = dx * cos + dy * sin;
    let local_y = -dx * sin + dy * cos;
    local_x.abs() <= wall.major_radius && local_y.abs() <= wall.minor_radius
}

const CLIMB_FACE_ALIGNMENT: f32 = 0.5;

fn oriented_boxes_overlap(
    centre_a: Vec2,
    axis_a: Vec2,
    half_a: (f32, f32),
    centre_b: Vec2,
    axis_b: Vec2,
    half_b: (f32, f32),
) -> bool {
    let perp = |axis: Vec2| Vec2::new(-axis.y, axis.x);
    let delta = centre_b.minus(centre_a);
    let extent = |axis: Vec2, box_axis: Vec2, half: (f32, f32)| {
        box_axis.dot(axis).abs() * half.0 + perp(box_axis).dot(axis).abs() * half.1
    };
    [axis_a, perp(axis_a), axis_b, perp(axis_b)]
        .into_iter()
        .all(|axis| {
            delta.dot(axis).abs() <= extent(axis, axis_a, half_a) + extent(axis, axis_b, half_b)
        })
}

fn arms_wall_climb(
    body_position: Vec2,
    body_facing: Facing,
    body_half: (f32, f32),
    wall: &mint_sage_passability::WallGeometry,
) -> bool {
    let body_axis = body_facing.to_vector();
    let (sin, cos) = mint_math::f32::sin_cos(wall.angle);
    let wall_axis = Vec2::new(cos, sin);
    if body_axis.dot(wall_axis).abs() < CLIMB_FACE_ALIGNMENT {
        return false;
    }
    oriented_boxes_overlap(
        body_position,
        body_axis,
        body_half,
        wall.centre,
        wall_axis,
        (wall.major_radius, wall.minor_radius),
    )
}

fn scales_walls(
    tick: &Tick<'_>,
    ids: HordeIds,
    kinds: &crate::templates::KindTable,
    entity: Entity,
) -> bool {
    tick.state()
        .get(ids.locomotion, entity)
        .map(|locomotion| locomotion.0)
        .or_else(|| {
            let kind = tick.state().get(ids.kind, entity)?;
            kinds.get(*kind)?.limits
        })
        .is_some_and(|kinematics| kinematics.scales_walls)
}

fn note_conditions(
    tick: &Tick<'_>,
    model_conditions: Option<ComponentId<mint_sage_modules::effects::UpgradeState>>,
    entity: Entity,
    effects: &mut Vec<(Entity, mint_sage_modules::effects::Effect)>,
    add: &[&str],
    remove: &[&str],
) {
    let adds: Vec<String> = add
        .iter()
        .filter(|name| !holds_condition(tick, model_conditions, entity, name))
        .map(|name| (*name).to_owned())
        .collect();
    let removes: Vec<String> = remove
        .iter()
        .filter(|name| holds_condition(tick, model_conditions, entity, name))
        .map(|name| (*name).to_owned())
        .collect();
    if !adds.is_empty() || !removes.is_empty() {
        effects.push((
            entity,
            mint_sage_modules::effects::Effect::ModelConditions {
                add: mint_sage_modules::effects::names(adds),
                remove: mint_sage_modules::effects::names(removes),
            },
        ));
    }
}

fn write_elevation(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    member: Entity,
    z: f32,
) -> mint_sim::Result<()> {
    match tick.state_mut().get_mut(ids.elevation, member) {
        Some(elevation) => elevation.0 = z,
        None => {
            tick.state_mut()
                .insert(ids.elevation, member, crate::unit::Elevation::new(z))?;
        }
    }
    Ok(())
}

fn push_wall_climb(tick: &mut Tick<'_>, ids: HordeIds, climber: Entity, wall: Entity) {
    let now = tick.frame();
    if let Some((_, facts)) = tick
        .state_mut()
        .store_mut(ids.combat_facts)
        .and_then(|store| store.iter_mut().next())
    {
        facts.push_wall_climb(now, climber, wall);
    }
}

pub(crate) fn advance_wall_climbers(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    heights: &HeightField,
    kinds: &crate::templates::KindTable,
    model_conditions: Option<ComponentId<mint_sage_modules::effects::UpgradeState>>,
) -> mint_sim::Result<Vec<(Entity, mint_sage_modules::effects::Effect)>> {
    let mut effects = Vec::new();
    for horde_entity in hordes(tick.state(), ids) {
        let Some(mut horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            continue;
        };
        if horde.climb_wall.is_none() {
            let Some(body) = tick.state().get(ids.body, horde_entity).copied() else {
                continue;
            };
            if !scales_walls(tick, ids, kinds, horde_entity) {
                continue;
            }
            let Some(geometry) = tick
                .state()
                .get(ids.kind, horde_entity)
                .and_then(|kind| kinds.get(*kind))
                .map(|entry| &entry.record.geometry)
            else {
                continue;
            };
            let major = geometry.major_radius.unwrap_or(0.0);
            let minor = geometry.minor_radius.unwrap_or(major);
            let found = crate::obstacles::scaleable_walls_of(tick.state(), ids.scaleable_walls)
                .and_then(|walls| {
                    walls.iter().find_map(|(entity, wall)| {
                        if Some(*entity) == horde.climb_wall
                            || tick.state().contains(ids.doomed, *entity)
                        {
                            return None;
                        }
                        arms_wall_climb(body.0.position, body.0.facing, (major, minor), wall)
                            .then_some(*entity)
                    })
                });
            let Some(wall_entity) = found else {
                continue;
            };
            horde.climb_wall = Some(wall_entity);
            horde.climb_quiet = 0;
            note_conditions(
                tick,
                model_conditions,
                horde_entity,
                &mut effects,
                &[SCALING_WALL],
                &[],
            );
            crate::diag::diag!(
                "climb: horde {} SCALING_WALL set (wall {})",
                horde_entity.index(),
                wall_entity.index()
            );
        }
        let Some(wall_entity) = horde.climb_wall else {
            continue;
        };
        let Some(wall) = crate::obstacles::scaleable_walls_of(tick.state(), ids.scaleable_walls)
            .and_then(|walls| {
                walls
                    .iter()
                    .find(|(entity, _)| *entity == wall_entity)
                    .map(|(_, wall)| *wall)
            })
        else {
            horde.climb_wall = None;
            horde.climb_quiet = 0;
            note_conditions(
                tick,
                model_conditions,
                horde_entity,
                &mut effects,
                &[],
                &[SCALING_WALL],
            );
            tick.state_mut().insert(ids.horde, horde_entity, horde)?;
            continue;
        };
        let wall_ground = heights.height_at_point(wall.centre).unwrap_or(0.0);
        let top = wall_ground + wall.height;
        let mut any_climbing = false;
        for member in horde.members.clone() {
            let Some(shape) = tick.state().get(ids.shape, member).copied() else {
                continue;
            };
            if !shape
                .kind_of
                .contains(mint_sage_asset::KindOf::CAN_CLIMB_WALLS)
            {
                continue;
            }
            let Some(position) = tick
                .state()
                .get(ids.body, member)
                .map(|body| body.0.position)
            else {
                continue;
            };
            let ground_here = heights.height_at_point(position).unwrap_or(wall_ground);
            let current_z = tick
                .state()
                .get(ids.elevation, member)
                .map_or(ground_here, |elevation| elevation.0);
            let limits = crate::unit::current_limits(
                tick.state(),
                kinds,
                (ids.kind, ids.locomotion, ids.health),
                member,
                None,
                1.0,
            );
            #[allow(clippy::cast_precision_loss)]
            let speed =
                limits.map_or(0.0, |limits| limits.max_speed) / mint_sim::SIMULATION_HZ as f32;
            let inside = wall_contains(position, &wall);
            if inside {
                let next_z = (current_z + speed * CLIMB_RATE_SHARE).min(top);
                write_elevation(tick, ids, member, next_z)?;
                note_conditions(
                    tick,
                    model_conditions,
                    member,
                    &mut effects,
                    &[CLIMBING, SCALING_WALL_HORDE],
                    &[RAPPELLING],
                );
                push_wall_climb(tick, ids, member, wall_entity);
                any_climbing = true;
                continue;
            }
            if current_z > ground_here + CLIMB_GROUNDED_TOLERANCE {
                let next_z = (current_z - speed * CLIMB_RATE_SHARE).max(ground_here);
                write_elevation(tick, ids, member, next_z)?;
                note_conditions(
                    tick,
                    model_conditions,
                    member,
                    &mut effects,
                    &[CLIMBING, SCALING_WALL_HORDE],
                    &[RAPPELLING],
                );
                push_wall_climb(tick, ids, member, wall_entity);
                any_climbing = true;
                continue;
            }
            if current_z != ground_here {
                write_elevation(tick, ids, member, ground_here)?;
            }
            let distance_to_slot = member_slot_pose(tick.state(), ids, member)
                .map_or(f32::INFINITY, |slot| slot.minus(position).length());
            if distance_to_slot > CLIMB_CLEAR_SLOT_DISTANCE {
                note_conditions(
                    tick,
                    model_conditions,
                    member,
                    &mut effects,
                    &[],
                    &[CLIMBING, RAPPELLING, SCALING_WALL_HORDE],
                );
            }
        }
        if any_climbing {
            horde.climb_quiet = 0;
        } else {
            horde.climb_quiet = horde.climb_quiet.saturating_add(1);
            if horde.climb_quiet >= CLIMB_CLEAR_QUIET_CHECKS {
                horde.climb_wall = None;
                horde.climb_quiet = 0;
                note_conditions(
                    tick,
                    model_conditions,
                    horde_entity,
                    &mut effects,
                    &[],
                    &[SCALING_WALL],
                );
                for member in horde.members.iter().copied() {
                    note_conditions(
                        tick,
                        model_conditions,
                        member,
                        &mut effects,
                        &[],
                        &[CLIMBING, RAPPELLING, SCALING_WALL_HORDE],
                    );
                }
                crate::diag::diag!("climb: horde {} SCALING_WALL cleared", horde_entity.index());
            }
        }
        tick.state_mut().insert(ids.horde, horde_entity, horde)?;
    }
    Ok(effects)
}

fn porcupine_facing(centre: Vec2, slot: Vec2, horde: Facing) -> Facing {
    Facing::from_vector(slot.minus(centre)).unwrap_or(horde)
}

pub(crate) fn reform(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    horde_entity: Entity,
    fits: &dyn Fn(Entity, u16) -> bool,
    stop_ranks: &[u32],
    is_aod: bool,
) -> mint_sim::Result<()> {
    let Some(mut horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
        return Ok(());
    };
    let Some(body) = tick.state().get(ids.body, horde_entity).copied() else {
        return Ok(());
    };
    let poses = slot_poses(&horde, &body.0);
    let mut waiting: Vec<(Entity, Vec2)> = Vec::new();
    for member in &horde.members {
        let Some(slot) = tick.state().get(ids.member, *member).and_then(|m| m.slot) else {
            continue;
        };
        if Some(slot) == horde.banner_slot {
            continue;
        }
        let Some(at) = tick.state().get(ids.body, *member).map(|b| b.0.position) else {
            continue;
        };
        horde.free.push(slot);
        waiting.push((*member, at));
    }
    let mut held: Vec<(Entity, u16)> = Vec::with_capacity(waiting.len());
    while !waiting.is_empty() {
        let mut best: Option<(usize, usize, f32)> = None;
        for (m, (member, at)) in waiting.iter().enumerate() {
            let mut nearest: Option<(usize, f32)> = None;
            for (f, slot) in horde.free.iter().enumerate() {
                if !fits(*member, *slot) {
                    continue;
                }
                let Some(pose) = poses.get(usize::from(*slot)) else {
                    continue;
                };
                let d = pose.minus(*at).length_squared();
                if nearest.is_none_or(|(_, n)| d < n) {
                    nearest = Some((f, d));
                }
            }
            let (f, d) = nearest.unwrap_or((0, 0.0));
            if best.is_none_or(|(_, _, b)| d > b) {
                best = Some((m, f, d));
            }
        }
        let Some((m, f, _)) = best else {
            break;
        };
        if f >= horde.free.len() {
            break;
        }
        let slot = horde.free.remove(f);
        let (member, _) = waiting.swap_remove(m);
        held.push((member, slot));
    }
    if !is_aod {
        while advance_into_free_slot(&horde.slots, &mut horde.free, stop_ranks, &mut held) {}
    }
    for (member, slot) in &held {
        let Some(entry) = tick.state().get(ids.member, *member).copied() else {
            continue;
        };
        if entry.slot != Some(*slot) {
            tick.state_mut().insert(
                ids.member,
                *member,
                Member {
                    slot: Some(*slot),
                    ..entry
                },
            )?;
        }
    }
    horde.dirty = true;
    tick.state_mut().insert(ids.horde, horde_entity, horde)?;
    Ok(())
}

pub(crate) fn prune_hordes(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    formations: &[FormationTable],
) -> mint_sim::Result<Vec<(Entity, Vec<Entity>)>> {
    let mut retired = Vec::new();
    for horde_entity in hordes(tick.state(), ids) {
        let Some(mut horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            continue;
        };
        let before = horde.members.len();
        let mut kept = Vec::with_capacity(before);
        for member in &horde.members {
            let member = *member;
            let dead = tick
                .state()
                .get(ids.health, member)
                .is_some_and(|health| health.0.is_dead());
            if tick.state().is_live(member) && !tick.state().contains(ids.doomed, member) && !dead {
                kept.push(member);
                continue;
            }
            let freed = tick.state().get(ids.member, member).and_then(|m| m.slot);
            if let Some(slot) = freed.filter(|slot| Some(*slot) != horde.banner_slot) {
                horde.free.insert(0, slot);
            }
        }
        if kept.len() == before {
            continue;
        }
        if kept.is_empty() {
            tick.state_mut().remove(ids.path, horde_entity);
            tick.state_mut().despawn(horde_entity);
            retired.push((horde_entity, horde.members));
            continue;
        }
        let table = formations.get(usize::from(horde.formation));
        let is_aod = table.is_some_and(|table| table.settings().aod.is_some());
        let stop_ranks = table.map_or(&[][..], |table| {
            table.settings().ranks_that_stop_advance.as_slice()
        });
        let mut held: Vec<(Entity, u16)> = kept
            .iter()
            .filter_map(|member| {
                let slot = tick.state().get(ids.member, *member)?.slot?;
                Some((*member, slot))
            })
            .collect();
        let moved_from: Vec<u16> = held.iter().map(|(_, slot)| *slot).collect();
        if !is_aod {
            while advance_into_free_slot(&horde.slots, &mut horde.free, stop_ranks, &mut held) {}
        }
        for ((member, slot), was) in held.iter().zip(moved_from) {
            if *slot != was {
                if let Some(mut current) = tick.state().get(ids.member, *member).copied() {
                    current.slot = Some(*slot);
                    tick.state_mut().insert(ids.member, *member, current)?;
                }
                horde.dirty = true;
            }
        }
        horde.members = kept;
        tick.state_mut().insert(ids.horde, horde_entity, horde)?;
    }
    drop_orphaned_members(tick.state_mut(), ids.member);
    Ok(retired)
}

pub(crate) fn drop_orphaned_members(world: &mut World, member: ComponentId<Member>) {
    let orphans: Vec<Entity> = world
        .store(member)
        .map(|store| {
            store
                .iter()
                .filter(|(_, held)| !world.is_live(held.horde))
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap_or_default();
    for orphan in orphans {
        world.remove(member, orphan);
    }
}

fn advance_into_free_slot(
    slots: &[HordeSlot],
    free: &mut Vec<u16>,
    stop_ranks: &[u32],
    held: &mut [(Entity, u16)],
) -> bool {
    let Some((at, front)) = free
        .iter()
        .copied()
        .enumerate()
        .min_by_key(|(_, slot)| *slot)
    else {
        return false;
    };
    let Some(target) = slots.get(usize::from(front)) else {
        return false;
    };
    let mut stop = stop_ranks
        .iter()
        .copied()
        .find(|rank| *rank >= target.rank)
        .unwrap_or(99);
    let mut best: Option<usize> = None;
    let mut best_distance = 99_999.0_f32;
    for (index, (_, slot)) in held.iter().enumerate() {
        if *slot <= front {
            continue;
        }
        let Some(candidate) = slots.get(usize::from(*slot)) else {
            continue;
        };
        if candidate.rank > stop || candidate.rank <= target.rank {
            continue;
        }
        let away = candidate.offset.minus(target.offset);
        let distance = away.x * away.x + away.y * away.y;
        if distance < best_distance {
            best = Some(index);
            best_distance = distance;
            stop = candidate.rank;
        }
    }
    let Some(chosen) = best.and_then(|index| held.get_mut(index)) else {
        return false;
    };
    free.remove(at);
    free.push(chosen.1);
    chosen.1 = front;
    true
}

fn assign_non_banner_slots(
    tick: &Tick<'_>,
    ids: HordeIds,
    horde: &Horde,
    poses: &[Vec2],
) -> Vec<(Entity, Option<u16>)> {
    let banner_slot = horde.banner_slot;
    let mut open_poses = Vec::with_capacity(poses.len());
    let mut open_slots = Vec::with_capacity(poses.len());
    for (index, pose) in poses.iter().enumerate() {
        let Ok(slot) = u16::try_from(index) else {
            continue;
        };
        if Some(slot) == banner_slot {
            continue;
        }
        open_poses.push(*pose);
        open_slots.push(slot);
    }
    let mut positions = Vec::with_capacity(horde.members.len());
    let mut members = Vec::with_capacity(horde.members.len());
    for member in &horde.members {
        let is_carrier = banner_slot.is_some()
            && tick.state().get(ids.member, *member).and_then(|m| m.slot) == banner_slot;
        if is_carrier {
            continue;
        }
        positions.push(
            tick.state()
                .get(ids.body, *member)
                .map_or(Vec2::ZERO, |state| state.0.position),
        );
        members.push(*member);
    }
    let assignment = assign_slots(&positions, &open_poses);
    members
        .into_iter()
        .enumerate()
        .map(|(index, member)| {
            let slot = assignment
                .slot_of_unit
                .get(index)
                .copied()
                .flatten()
                .and_then(|slot| open_slots.get(usize::from(slot)).copied());
            (member, slot)
        })
        .collect()
}

pub(crate) fn reassign_slots(tick: &mut Tick<'_>, ids: HordeIds) -> mint_sim::Result<()> {
    for horde_entity in hordes(tick.state(), ids) {
        let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            continue;
        };
        let Some(body) = tick.state().get(ids.body, horde_entity).copied() else {
            continue;
        };
        let poses = slot_poses(&horde, &body.0);
        let assignments = assign_non_banner_slots(tick, ids, &horde, &poses);
        let mut held: Vec<u16> = Vec::with_capacity(assignments.len());
        for (member, slot) in assignments {
            held.extend(slot);
            tick.state_mut().insert(
                ids.member,
                member,
                Member {
                    horde: horde_entity,
                    slot,
                    parked: false,
                },
            )?;
        }
        let mut rebuilt = horde;
        rebuilt.free = free_slots(&rebuilt, &held);
        rebuilt.dirty = true;
        tick.state_mut().insert(ids.horde, horde_entity, rebuilt)?;
    }
    Ok(())
}

fn free_slots(horde: &Horde, held: &[u16]) -> Vec<u16> {
    (0..u16::try_from(horde.slots.len()).unwrap_or(u16::MAX))
        .filter(|slot| Some(*slot) != horde.banner_slot && !held.contains(slot))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BannerPlan {
    pub(crate) kind: crate::unit::Kind,
    pub(crate) offset: Vec2,
    pub(crate) min_level: u32,
    pub(crate) destroy_horde_on_death: bool,
}

pub(crate) fn banner_plans(
    formations: &crate::combat::FormationLibrary,
    kinds: &crate::templates::KindTable,
) -> Vec<Option<BannerPlan>> {
    formations
        .banners()
        .iter()
        .map(|banner| {
            let banner = banner.as_ref()?;
            Some(BannerPlan {
                kind: kinds.kind(&banner.template)?,
                offset: Vec2::new(banner.offset.0, banner.offset.1),
                min_level: banner.min_level,
                destroy_horde_on_death: banner.destroy_horde_on_death,
            })
        })
        .collect()
}

pub(crate) fn enlist_banner(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    horde_entity: Entity,
    banner: Entity,
    offset: Vec2,
) -> mint_sim::Result<()> {
    let Some(mut horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
        return Ok(());
    };
    let slot = u16::try_from(horde.slots.len()).ok();
    horde.slots.push(HordeSlot {
        offset,
        rank: 0,
        unit_type: None,
        index_in_rank: 0,
        leader: None,
    });
    horde.members.push(banner);
    horde.banner_slot = slot;
    horde.dirty = true;
    tick.state_mut().insert(
        ids.member,
        banner,
        Member {
            horde: horde_entity,
            slot,
            parked: false,
        },
    )?;
    tick.state_mut()
        .insert(ids.member_mark, banner, HordeMemberMark)?;
    tick.state_mut().insert(ids.horde, horde_entity, horde)?;
    Ok(())
}

fn formation_centre(
    table: &FormationTable,
    offsets: &[Vec2],
    leader_offset: Option<Vec2>,
    banner_offset: Option<Vec2>,
    is_machine: impl Fn(&str) -> bool,
) -> Vec2 {
    for (index, slot) in table.slots().iter().enumerate() {
        let Some(name) = table.unit_types().get(usize::from(slot.unit_type)) else {
            continue;
        };
        if is_machine(name) {
            if let Some(offset) = offsets.get(index) {
                return *offset;
            }
        }
    }
    let mut sum = Vec2::ZERO;
    let mut count: u32 = 0;
    for offset in offsets {
        sum = sum.plus(*offset);
        count = count.saturating_add(1);
    }
    for extra in [leader_offset, banner_offset].into_iter().flatten() {
        sum = sum.plus(extra);
        count = count.saturating_add(1);
    }
    if count == 0 {
        return Vec2::ZERO;
    }
    #[allow(clippy::cast_precision_loss)]
    let divisor = count as f32;
    Vec2::new(sum.x / divisor, sum.y / divisor)
}

pub(crate) fn spawn_horde(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    spawn: &HordeSpawn,
    table: &FormationTable,
    banner_offset: Option<Vec2>,
    is_machine: impl Fn(&str) -> bool,
    passability: &mint_sage_passability::Passability,
    mut insert_member: impl FnMut(&mut Tick<'_>, &Spawn, Vec2, Facing) -> mint_sim::Result<Entity>,
) -> mint_sim::Result<Entity> {
    let drawn = table.place(tick.rng());
    let centre = formation_centre(
        table,
        &drawn,
        table.leader_offset(),
        banner_offset,
        is_machine,
    );
    let offsets: Vec<Vec2> = drawn.iter().map(|offset| offset.minus(centre)).collect();
    let horde_entity = tick.state_mut().spawn()?;
    tick.state_mut().insert(
        ids.body,
        horde_entity,
        Body(MoverState::new(spawn.origin, spawn.facing)),
    )?;

    let basis = FormationBasis::of(spawn.facing);
    let wanted = usize::try_from(spawn.count)
        .unwrap_or(usize::MAX)
        .min(offsets.len());
    let mut members = Vec::with_capacity(wanted);
    let mut positions = Vec::with_capacity(wanted);
    for offset in offsets.iter().take(wanted) {
        let pose = spawn_pose(
            passability,
            spawn.origin,
            basis.place(spawn.origin, *offset),
        );
        let entity = insert_member(tick, &spawn.member, pose, spawn.facing)?;
        members.push(entity);
        positions.push(pose);
    }

    let poses: Vec<Vec2> = offsets
        .iter()
        .map(|offset| basis.place(spawn.origin, *offset))
        .collect();
    let assignment = assign_slots(&positions, &poses);
    let mut held = Vec::with_capacity(members.len());
    let fade_in = table.settings().aod.is_some().then_some(JOIN_FADE_DURATION);
    for (index, member) in members.iter().enumerate() {
        let slot = assignment.slot_of_unit.get(index).copied().flatten();
        held.push(slot);
        tick.state_mut().insert(
            ids.member,
            *member,
            Member {
                horde: horde_entity,
                slot,
                parked: false,
            },
        )?;
        tick.state_mut()
            .insert(ids.member_mark, *member, HordeMemberMark)?;
        if let Some(duration) = fade_in {
            let now = tick.frame();
            tick.state_mut()
                .insert(ids.join_fade, *member, JoinFade::starting(now, duration))?;
        }
    }

    let slots: Vec<HordeSlot> = offsets
        .iter()
        .zip(table.slots())
        .map(|(offset, slot)| HordeSlot {
            offset: *offset,
            rank: slot.rank,
            unit_type: Some(slot.unit_type),
            index_in_rank: slot.index_in_rank,
            leader: slot.leader,
        })
        .collect();
    attach_trail(
        tick,
        ids,
        horde_entity,
        table,
        &slots,
        spawn.origin,
        spawn.facing,
    )?;
    let mut horde = Horde {
        formation: spawn.formation,
        slots,
        members,
        free: Vec::new(),
        banner_slot: None,
        dirty: true,
        centre,
        climb_wall: None,
        climb_quiet: 0,
    };
    let held: Vec<u16> = held.into_iter().flatten().collect();
    horde.free = free_slots(&horde, &held);
    tick.state_mut().insert(ids.horde, horde_entity, horde)?;
    Ok(horde_entity)
}

fn spawn_pose(passability: &mint_sage_passability::Passability, origin: Vec2, slot: Vec2) -> Vec2 {
    let stands = |at: Vec2| {
        passability.cell_at(at).is_none_or(|cell| {
            mint_movement::Passability::is_passable(&passability.grid, cell, GROUND)
        })
    };
    if stands(slot) {
        return slot;
    }
    (1..=SLOT_CLAMP_CANDIDATES)
        .map(|step| {
            #[allow(clippy::cast_precision_loss)]
            let t = step as f32 / 16.0;
            Vec2::new(
                slot.x + (origin.x - slot.x) * t,
                slot.y + (origin.y - slot.y) * t,
            )
        })
        .find(|&candidate| stands(candidate))
        .unwrap_or_else(|| crate::skirmish::standable(passability, origin))
}

fn attach_trail(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    horde_entity: Entity,
    table: &FormationTable,
    slots: &[HordeSlot],
    origin: Vec2,
    facing: Facing,
) -> mint_sim::Result<()> {
    let Some(sway) = table.settings().aod else {
        return Ok(());
    };
    let trail = crate::horde_trail::HordeTrail::new(sway, slots, origin, facing, tick.rng());
    tick.state_mut()
        .insert(ids.horde_trail, horde_entity, trail)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProductionStream {
    pub(crate) member: Spawn,
    pub(crate) remaining: u32,
    pub(crate) next_at: mint_sim::Frame,
    pub(crate) delay: mint_sim::LogicFrames,
    pub(crate) create: Vec2,
    pub(crate) midpoint: Vec2,
    pub(crate) facing: Facing,
    pub(crate) source: Entity,
}

impl Component for ProductionStream {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.member.hash_into(hasher);
        hasher.write_u32(self.remaining);
        self.next_at.hash_into(hasher);
        hasher.write_u32(self.delay.get());
        hasher.write_f32(self.create.x);
        hasher.write_f32(self.create.y);
        hasher.write_f32(self.midpoint.x);
        hasher.write_f32(self.midpoint.y);
        hasher.write_f32(self.facing.radians());
        self.source.hash_into(hasher);
    }
}

pub(crate) fn spawn_horde_shell(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    formation: u16,
    origin: Vec2,
    facing: Facing,
    table: &FormationTable,
    banner_offset: Option<Vec2>,
    is_machine: impl Fn(&str) -> bool,
) -> mint_sim::Result<Entity> {
    let drawn = table.place(tick.rng());
    let centre = formation_centre(
        table,
        &drawn,
        table.leader_offset(),
        banner_offset,
        is_machine,
    );
    let offsets: Vec<Vec2> = drawn.iter().map(|offset| offset.minus(centre)).collect();
    let horde_entity = tick.state_mut().spawn()?;
    tick.state_mut().insert(
        ids.body,
        horde_entity,
        Body(MoverState::new(origin, facing)),
    )?;
    let slots: Vec<HordeSlot> = offsets
        .iter()
        .zip(table.slots())
        .map(|(offset, slot)| HordeSlot {
            offset: *offset,
            rank: slot.rank,
            unit_type: Some(slot.unit_type),
            index_in_rank: slot.index_in_rank,
            leader: slot.leader,
        })
        .collect();
    attach_trail(tick, ids, horde_entity, table, &slots, origin, facing)?;
    let mut horde = Horde {
        formation,
        slots,
        members: Vec::new(),
        free: Vec::new(),
        banner_slot: None,
        dirty: true,
        centre,
        climb_wall: None,
        climb_quiet: 0,
    };
    horde.free = free_slots(&horde, &[]);
    tick.state_mut().insert(ids.horde, horde_entity, horde)?;
    Ok(horde_entity)
}

pub(crate) fn exit_one_member(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    horde_entity: Entity,
    at: Vec2,
    facing: Facing,
    slot_accepts: impl Fn(Option<u16>) -> bool,
    mut insert_member: impl FnMut(&mut Tick<'_>, Vec2, Facing) -> mint_sim::Result<Entity>,
) -> mint_sim::Result<Option<Entity>> {
    let Some(mut horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
        return Ok(None);
    };
    let Some(at_free) = horde.free.iter().position(|&slot| {
        horde
            .slots
            .get(usize::from(slot))
            .is_some_and(|slot| slot_accepts(slot.unit_type))
    }) else {
        return Ok(None);
    };
    let Some(&slot) = horde.free.get(at_free) else {
        return Ok(None);
    };
    let entity = insert_member(tick, at, facing)?;
    tick.state_mut().insert(
        ids.member,
        entity,
        Member {
            horde: horde_entity,
            slot: Some(slot),
            parked: false,
        },
    )?;
    tick.state_mut()
        .insert(ids.member_mark, entity, HordeMemberMark)?;
    horde.free.remove(at_free);
    horde.members.push(entity);
    horde.dirty = true;
    tick.state_mut().insert(ids.horde, horde_entity, horde)?;
    Ok(Some(entity))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BurstGather {
    KeepRoutes,
    ClearOfBuilding { producer: Vec2, clear_radius: f32 },
}

pub(crate) fn finish_burst(
    tick: &mut Tick<'_>,
    ids: HordeIds,
    horde_entity: Entity,
    gather: BurstGather,
    kinds: &crate::templates::KindTable,
    destination: Option<Vec2>,
) -> mint_sim::Result<()> {
    if let BurstGather::ClearOfBuilding {
        producer,
        clear_radius,
    } = gather
    {
        let (Some(horde), Some(body)) = (
            tick.state().get(ids.horde, horde_entity).cloned(),
            tick.state().get(ids.body, horde_entity).map(|body| body.0),
        ) else {
            return Ok(());
        };
        let poses = slot_poses(&horde, &body);
        for member in &horde.members {
            let Some(slot) = tick
                .state()
                .get(ids.member, *member)
                .and_then(|entry| entry.slot)
                .and_then(|slot| poses.get(usize::from(slot)).copied())
            else {
                continue;
            };
            let Some(position) = tick
                .state()
                .get(ids.body, *member)
                .map(|body| body.0.position)
            else {
                continue;
            };
            let settle = slot_arrive(&member_limits(tick, ids, *member, kinds), false, 0.0);
            if slot.minus(position).length() < settle {
                continue;
            }
            if producer.minus(position).length() <= clear_radius {
                if let Some(mut path) = tick.state().get(ids.path, *member).cloned() {
                    if let Some(end) = path.route.points.last_mut() {
                        *end = slot;
                        path.goal = slot;
                        tick.state_mut().insert(ids.path, *member, path)?;
                    }
                }
                continue;
            }
            crate::unit::give_path(
                tick.state_mut(),
                (ids.body, ids.path),
                *member,
                crate::unit::Path {
                    route: mint_movement::Route::to_point(slot),
                    goal: slot,
                    kind: crate::unit::PathKind::Ordered,
                    facing: None,
                },
            )?;
        }
    }
    if let Some(destination) = destination {
        crate::unit::give_path(
            tick.state_mut(),
            (ids.body, ids.path),
            horde_entity,
            crate::unit::Path {
                route: mint_movement::Route::to_point(destination),
                goal: destination,
                kind: crate::unit::PathKind::Ordered,
                facing: None,
            },
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HordeBlock {
    pub(crate) centre: Vec2,
    pub(crate) half_along: f32,
    pub(crate) half_across: f32,
    pub(crate) member_radius: f32,
}

pub(crate) fn horde_block(world: &World, ids: HordeIds, horde: Entity) -> Option<HordeBlock> {
    let table = world.get(ids.horde, horde)?;
    let mut bounds: Option<(Vec2, Vec2)> = None;
    let mut member_radius = 0.0f32;
    for member in &table.members {
        if let Some(shape) = world.get(ids.shape, *member) {
            member_radius = member_radius.max(shape.radius);
        }
        let Some(slot) = world.get(ids.member, *member).and_then(|m| m.slot) else {
            continue;
        };
        let Some(offset) = table.offset(slot) else {
            continue;
        };
        bounds = Some(bounds.map_or((offset, offset), |(lo, hi)| {
            (
                Vec2::new(lo.x.min(offset.x), lo.y.min(offset.y)),
                Vec2::new(hi.x.max(offset.x), hi.y.max(offset.y)),
            )
        }));
    }
    let (lo, hi) = bounds?;
    Some(HordeBlock {
        centre: lo.plus(hi).scale(0.5),
        half_along: (hi.x - lo.x) * 0.5 + member_radius,
        half_across: (hi.y - lo.y) * 0.5 + member_radius,
        member_radius,
    })
}

pub(crate) fn hordes_of(world: &World, ids: HordeIds, selection: &[Entity]) -> Vec<Entity> {
    let mut found: Vec<Entity> = Vec::new();
    for entity in selection {
        let Some(member) = world.get(ids.member, *entity) else {
            continue;
        };
        if !world.is_live(member.horde) {
            continue;
        }
        if let Err(at) = found.binary_search_by_key(&member.horde.index(), |entity| entity.index())
        {
            found.insert(at, member.horde);
        }
    }
    found
}

#[must_use]
pub(crate) fn crowding(world: &World, ids: HordeIds, horde_entity: Entity) -> Option<(f32, f32)> {
    let horde = world.get(ids.horde, horde_entity)?;
    let body = world.get(ids.body, horde_entity)?.0;
    let poses = slot_poses(horde, &body);
    let mut placed: Vec<(Vec2, Option<Vec2>)> = Vec::with_capacity(horde.members.len());
    for member in &horde.members {
        let Some(state) = world.get(ids.body, *member) else {
            continue;
        };
        let slot = world
            .get(ids.member, *member)
            .and_then(|entry| entry.slot)
            .and_then(|slot| poses.get(usize::from(slot)).copied());
        placed.push((state.0.position, slot));
    }
    if placed.len() < 2 {
        return None;
    }
    let mut closest = f32::MAX;
    for (index, (position, _)) in placed.iter().enumerate() {
        for (other, _) in placed.iter().skip(index.saturating_add(1)) {
            closest = closest.min(other.minus(*position).length());
        }
    }
    let mut error = 0.0f32;
    let mut counted = 0u16;
    for (position, slot) in &placed {
        if let Some(slot) = slot {
            error += slot.minus(*position).length();
            counted = counted.saturating_add(1);
        }
    }
    let mean = if counted == 0 {
        0.0
    } else {
        error / f32::from(counted)
    };
    Some((closest, mean))
}

#[must_use]
pub(crate) fn footprint(world: &World, ids: HordeIds, horde_entity: Entity) -> (f32, f32) {
    let Some(horde) = world.get(ids.horde, horde_entity) else {
        return (0.0, 0.0);
    };
    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    let mut any = false;
    for member in &horde.members {
        let Some(body) = world.get(ids.body, *member) else {
            continue;
        };
        any = true;
        min_x = min_x.min(body.0.position.x);
        min_y = min_y.min(body.0.position.y);
        max_x = max_x.max(body.0.position.x);
        max_y = max_y.max(body.0.position.y);
    }
    if !any {
        return (0.0, 0.0);
    }
    ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
}

pub(crate) fn report(world: &World, ids: HordeIds) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(store) = world.store(ids.horde) else {
        return lines;
    };
    for (entity, horde) in store.iter() {
        let member_count = horde
            .members
            .iter()
            .filter(|member| world.get(ids.body, **member).is_some())
            .count();
        let (span_x, span_y) = footprint(world, ids, entity);
        let body = world.get(ids.body, entity).map(|body| body.0);
        let (closest, slot_error) = crowding(world, ids, entity).unwrap_or((0.0, 0.0));
        lines.push(format!(
            "horde {}: {} slot(s), {} member(s), spanning {:.0} x {:.0} world units, \
             closest pair {closest:.1}, mean slot error {slot_error:.1}, \
             at ({:.0}, {:.0}) facing {:.2} rad, {}",
            entity.index(),
            horde.slots.len(),
            member_count,
            span_x,
            span_y,
            body.map_or(0.0, |body| body.position.x),
            body.map_or(0.0, |body| body.position.y),
            body.map_or(0.0, |body| body.facing.radians()),
            if world.contains(ids.path, entity) {
                "moving"
            } else {
                "stopped"
            }
        ));
    }
    lines
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_formation::{HordeContainDef, RankLine, SlotPosition};

    fn climb_wall(centre: Vec2) -> mint_sage_passability::WallGeometry {
        mint_sage_passability::WallGeometry {
            id: 0,
            centre,
            angle: 0.0,
            major_radius: 10.0,
            minor_radius: 100.0,
            height: 30.0,
        }
    }

    #[test]
    fn a_box_body_arms_within_the_combined_major_radii_but_not_past_them() {
        let body_half = (40.0, 50.0);
        assert!(arms_wall_climb(
            Vec2::ZERO,
            Facing::EAST,
            body_half,
            &climb_wall(Vec2::new(49.0, 0.0))
        ));
        assert!(!arms_wall_climb(
            Vec2::ZERO,
            Facing::EAST,
            body_half,
            &climb_wall(Vec2::new(51.0, 0.0))
        ));
    }

    #[test]
    fn a_body_past_sixty_degrees_off_the_wall_s_face_never_arms() {
        let facing = Facing::from_radians(70.0_f32.to_radians());
        assert!(!arms_wall_climb(
            Vec2::ZERO,
            facing,
            (40.0, 50.0),
            &climb_wall(Vec2::new(30.0, 0.0))
        ));
    }

    fn rank(number: u32, unit_type: &str, xs: &[f32]) -> RankLine {
        RankLine {
            rank: number,
            unit_type: unit_type.to_owned(),
            positions: xs
                .iter()
                .map(|x| SlotPosition {
                    x: *x,
                    y: 0.0,
                    leader: None,
                })
                .collect(),
        }
    }

    #[test]
    fn the_centre_is_the_mean_of_rank_slots_and_the_banner_slot() {
        let def = HordeContainDef {
            slots: Some(4),
            ranks: vec![rank(1, "U", &[50.0, 50.0]), rank(2, "U", &[10.0, 10.0])],
            ..HordeContainDef::default()
        };
        let table = FormationTable::from_contain(&def);
        let offsets = table.place(&mut mint_math::rng::Pcg32::new(1));
        let banner = Some(Vec2::new(70.0, 0.0));
        let centre = formation_centre(&table, &offsets, table.leader_offset(), banner, |_| false);
        assert!((centre.x - 38.0).abs() < 1e-5, "{centre:?}");

        let without_banner = formation_centre(&table, &offsets, None, None, |_| false);
        assert!((without_banner.x - 30.0).abs() < 1e-5, "{without_banner:?}");
    }

    #[test]
    fn a_machine_ranks_first_slot_is_the_centre_instead_of_the_mean() {
        let def = HordeContainDef {
            slots: Some(3),
            ranks: vec![rank(1, "SiegeMachine", &[5.0, 5.0]), rank(2, "U", &[10.0])],
            ..HordeContainDef::default()
        };
        let table = FormationTable::from_contain(&def);
        let offsets = table.place(&mut mint_math::rng::Pcg32::new(1));
        let centre = formation_centre(&table, &offsets, None, None, |name| name == "SiegeMachine");
        assert_eq!(centre, Vec2::new(5.0, 0.0));
    }

    fn slots_at(offsets: &[Vec2]) -> Vec<HordeSlot> {
        offsets
            .iter()
            .enumerate()
            .map(|(index, offset)| HordeSlot {
                offset: *offset,
                rank: 1,
                unit_type: Some(0),
                index_in_rank: u32::try_from(index).unwrap(),
                leader: None,
            })
            .collect()
    }

    #[test]
    fn every_field_of_a_horde_and_a_member_changes_its_hash() {
        let mut world = World::new();
        let a = world.spawn().unwrap();
        let b = world.spawn().unwrap();
        let hash_of = |value: &dyn Fn(&mut StateHasher)| {
            let mut hasher = StateHasher::new();
            value(&mut hasher);
            hasher.finish()
        };
        let base = Horde {
            formation: 0,
            slots: slots_at(&[Vec2::new(1.0, 2.0)]),
            members: vec![a],
            free: Vec::new(),
            dirty: false,
            banner_slot: None,
            centre: Vec2::ZERO,
            climb_wall: None,
            climb_quiet: 0,
        };
        let mut other_formation = base.clone();
        other_formation.formation = 1;
        let mut other_offset = base.clone();
        other_offset.slots = slots_at(&[Vec2::new(1.0, 3.0)]);
        let mut more_offsets = base.clone();
        more_offsets.slots.extend(slots_at(&[Vec2::ZERO]));
        let mut other_member = base.clone();
        other_member.members = vec![b];
        let mut more_members = base.clone();
        more_members.members.push(b);
        let mut other_banner_slot = base.clone();
        other_banner_slot.banner_slot = Some(0);
        let mut other_free = base.clone();
        other_free.free = vec![0];
        let mut other_rank = base.clone();
        other_rank.slots[0].rank = 2;
        let mut other_index_in_rank = base.clone();
        other_index_in_rank.slots[0].index_in_rank = 1;
        let mut other_leader = base.clone();
        other_leader.slots[0].leader = Some(mint_sage_formation::LeaderRef { rank: 1, index: 0 });
        let mut other_dirty = base.clone();
        other_dirty.dirty = true;
        let mut other_centre = base.clone();
        other_centre.centre = Vec2::new(1.0, 0.0);
        for other in [
            other_formation,
            other_offset,
            more_offsets,
            other_member,
            more_members,
            other_banner_slot,
            other_free,
            other_rank,
            other_index_in_rank,
            other_leader,
            other_dirty,
            other_centre,
        ] {
            assert_ne!(
                hash_of(&|h| base.hash_into(h)),
                hash_of(&|h| other.hash_into(h)),
                "{other:?}"
            );
        }

        let member = Member {
            horde: a,
            slot: Some(3),
            parked: false,
        };
        for other in [
            Member {
                horde: b,
                slot: Some(3),
                parked: false,
            },
            Member {
                horde: a,
                slot: Some(4),
                parked: false,
            },
            Member {
                horde: a,
                slot: None,
                parked: false,
            },
            Member {
                horde: a,
                slot: Some(3),
                parked: true,
            },
        ] {
            assert_ne!(
                hash_of(&|h| member.hash_into(h)),
                hash_of(&|h| other.hash_into(h)),
                "{other:?}"
            );
        }
    }

    fn wall_grid(width: u32, height: u32, wall_x: u32) -> mint_sage_passability::Passability {
        let mut samples = vec![0u16; (width * height) as usize];
        for y in 0..height {
            if let Some(sample) = samples.get_mut((y * width + wall_x) as usize) {
                *sample = 400;
            }
        }
        let height_map = mint_sage_map::HeightMapData {
            width,
            height,
            border_size: 0,
            boundaries: Vec::new(),
            samples,
        };
        mint_sage_passability::Passability::build(&height_map, 5.0 / 128.0).unwrap()
    }

    #[test]
    fn a_blocked_spawn_slot_slides_toward_the_horde_not_past_the_cliff() {
        let passability = wall_grid(16, 5, 6);
        let geometry = passability.geometry;
        let centre = |x: u16| geometry.centre_of(mint_movement::Cell::new(x, 2));
        let (origin, slot) = (centre(1), centre(7));
        let wall = |at: Vec2| {
            let cell = passability.cell_at(at).unwrap();
            !mint_movement::Passability::is_passable(&passability.grid, cell, GROUND)
        };
        assert!(wall(slot), "the slot must start on the wall");
        let pose = spawn_pose(&passability, origin, slot);
        assert!(!wall(pose), "{pose:?} still on the wall");
        assert!(pose.x < centre(4).x, "{pose:?} crossed the wall");
        assert!(
            (pose.y - slot.y).abs() < 1.0e-3,
            "{pose:?} left the line to the horde"
        );
    }

    #[test]
    fn a_step_into_blocked_ground_is_valid_only_inside_the_goals_own_cell() {
        let passability = wall_grid(12, 5, 6);
        let view = mint_sage_passability::GatedView {
            ground: DynamicView {
                grid: &passability.grid,
                counts: &[],
                rubble: &[],
                wall_rim: &[],
                tags: &[],
                connect: &[],
                released: &[],
                closed: &[],
                obstacle_pinched: &[],
                ramp_side: &[],
                scaleable: &[],
                barred: &[],
            },
            portals: &[],
            tags: &[],
        };
        let geometry = passability.geometry;
        let centre = |x: u16| geometry.centre_of(mint_movement::Cell::new(x, 2));
        let (from, blocked, beyond) = (centre(2), centre(6), centre(9));
        assert!(
            !view.is_passable(mint_movement::Cell::new(6, 2), GROUND),
            "the wall cell must start blocked"
        );
        let goal_there = step_valid(&view, geometry, GROUND, (from, blocked, blocked));
        let goal_beyond = step_valid(&view, geometry, GROUND, (from, blocked, beyond));
        assert_eq!((goal_there, goal_beyond), (true, false));
    }

    fn flat_grid(
        width: u16,
        height: u16,
    ) -> (
        CostGrid,
        CellGeometry,
        mint_movement::GridExtent,
        HeightField,
    ) {
        let extent = mint_movement::GridExtent::new(width, height).unwrap();
        let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
        let grid = CostGrid::new(extent);
        let node_extent = mint_movement::GridExtent::new(width + 1, height + 1).unwrap();
        let samples = vec![0.0; usize::from(width + 1) * usize::from(height + 1)];
        let heights = HeightField::new(node_extent, geometry, samples).unwrap();
        (grid, geometry, extent, heights)
    }

    #[test]
    fn a_slot_over_an_obstacle_cell_clamps_toward_the_horde_centre() {
        let (grid, geometry, extent, heights) = flat_grid(10, 10);
        let mut counts = vec![0u8; usize::from(extent.width) * usize::from(extent.height)];
        let obstacle_cell = Cell::new(5, 5);
        counts[usize::from(obstacle_cell.y) * usize::from(extent.width)
            + usize::from(obstacle_cell.x)] = 1;
        let view = mint_sage_passability::GatedView {
            ground: DynamicView {
                grid: &grid,
                counts: &counts,
                rubble: &[],
                wall_rim: &[],
                tags: &[],
                connect: &[],
                released: &[],
                closed: &[],
                obstacle_pinched: &[],
                ramp_side: &[],
                scaleable: &[],
                barred: &[],
            },
            portals: &[],
            tags: &[],
        };
        let reachability = Reachability::build(&view.ground, GROUND);
        let footprint = crate::goal_cells::Footprint::of(2.0, false, geometry.cell_size(), None);
        let slot = geometry.centre_of(obstacle_cell);
        let horde_pos = geometry.centre_of(Cell::new(2, 5));
        let horde_cell = geometry.cell_at(horde_pos, 0, extent);
        assert!(
            !view.is_passable(obstacle_cell, GROUND),
            "the stamped cell must start blocked"
        );
        let clamped = clamp_member_slot(
            &view,
            geometry,
            &heights,
            &reachability,
            GROUND,
            footprint,
            horde_cell,
            horde_pos,
            slot,
        );
        assert_ne!(clamped, slot, "the raw slot sits on the obstacle");
        assert_ne!(
            geometry.cell_at(clamped, 0, extent),
            Some(obstacle_cell),
            "the clamped slot must leave the obstacle cell"
        );
        assert!(
            clamped.x < slot.x,
            "the clamp slides toward the horde centre, west of the slot"
        );
    }

    #[test]
    fn a_slot_on_a_height_step_over_ten_clamps_off_the_step() {
        let (grid, geometry, extent, _) = flat_grid(8, 5);
        let node_extent = mint_movement::GridExtent::new(9, 6).unwrap();
        let mut samples = vec![0.0f32; 9 * 6];
        for y in 0..6usize {
            for x in 0..9usize {
                if x >= 5 {
                    samples[y * 9 + x] = 40.0;
                }
            }
        }
        let heights = HeightField::new(node_extent, geometry, samples).unwrap();
        let view = mint_sage_passability::GatedView {
            ground: DynamicView {
                grid: &grid,
                counts: &[],
                rubble: &[],
                wall_rim: &[],
                tags: &[],
                connect: &[],
                released: &[],
                closed: &[],
                obstacle_pinched: &[],
                ramp_side: &[],
                scaleable: &[],
                barred: &[],
            },
            portals: &[],
            tags: &[],
        };
        let reachability = Reachability::build(&view.ground, GROUND);
        let footprint = crate::goal_cells::Footprint::of(20.0, false, geometry.cell_size(), None);
        let slot = Vec2::new(50.0, 25.0);
        let horde_pos = Vec2::new(20.0, 25.0);
        let horde_cell = geometry.cell_at(horde_pos, 0, extent);
        let clamped = clamp_member_slot(
            &view,
            geometry,
            &heights,
            &reachability,
            GROUND,
            footprint,
            horde_cell,
            horde_pos,
            slot,
        );
        assert_ne!(clamped, slot, "the raw slot's footprint straddles the step");
        assert!(
            clamped.x < 30.0,
            "the clamp must cross to the flat side of the step, got {clamped:?}"
        );
    }

    #[test]
    fn a_slot_with_no_passable_candidate_falls_back_to_the_horde_cell_centre() {
        let (grid, geometry, extent, heights) = flat_grid(8, 8);
        let counts = vec![1u8; usize::from(extent.width) * usize::from(extent.height)];
        let view = mint_sage_passability::GatedView {
            ground: DynamicView {
                grid: &grid,
                counts: &counts,
                rubble: &[],
                wall_rim: &[],
                tags: &[],
                connect: &[],
                released: &[],
                closed: &[],
                obstacle_pinched: &[],
                ramp_side: &[],
                scaleable: &[],
                barred: &[],
            },
            portals: &[],
            tags: &[],
        };
        let reachability = Reachability::build(&view.ground, GROUND);
        let footprint = crate::goal_cells::Footprint::of(2.0, false, geometry.cell_size(), None);
        let slot = geometry.centre_of(Cell::new(6, 6));
        let horde_pos = geometry.centre_of(Cell::new(2, 2));
        let horde_cell = geometry.cell_at(horde_pos, 0, extent);
        let clamped = clamp_member_slot(
            &view,
            geometry,
            &heights,
            &reachability,
            GROUND,
            footprint,
            horde_cell,
            horde_pos,
            slot,
        );
        assert_eq!(
            clamped,
            geometry.centre_of(horde_cell.unwrap()),
            "every cell is an obstacle, so the fallback must be the horde's own cell centre"
        );
    }

    #[test]
    fn a_climbing_movers_own_terrain_decides_reachability_across_a_cliff() {
        let passability = wall_grid(12, 5, 6);
        let view = mint_sage_passability::GatedView {
            ground: DynamicView {
                grid: &passability.grid,
                counts: &[],
                rubble: &[],
                wall_rim: &[],
                tags: &[],
                connect: &[],
                released: &[],
                closed: &[],
                obstacle_pinched: &[],
                ramp_side: &[],
                scaleable: &[],
                barred: &[],
            },
            portals: &[],
            tags: &[],
        };
        let geometry = passability.geometry;
        let extent = view.extent();
        let ground = Reachability::build(&view.ground, GROUND);
        let climber = mint_sage_passability::mover_class(
            &[String::from("GROUND"), String::from("CLIFF")],
            false,
        )
        .unwrap();
        let wide = Reachability::build(&view.ground, climber);
        let extra = vec![(climber.terrain(), wide)];
        let reachability = reachability_for(&ground, &extra, climber);
        assert_eq!(
            reachability.mover(),
            climber.terrain(),
            "the lookup must hand back the climbing oracle built for it, not GROUND's own"
        );
        let footprint = crate::goal_cells::Footprint::of(2.0, false, geometry.cell_size(), None);
        let slot = geometry.centre_of(Cell::new(2, 2));
        let horde_pos = geometry.centre_of(Cell::new(9, 2));
        let horde_cell = geometry.cell_at(horde_pos, 0, extent);
        assert!(
            !ground.same_component(
                geometry.cell_at(slot, 0, extent).unwrap(),
                horde_cell.unwrap()
            ),
            "the cliff wall must split the grid for GROUND"
        );
        let clamped = clamp_member_slot(
            &view,
            geometry,
            &passability.heights,
            reachability,
            climber,
            footprint,
            horde_cell,
            horde_pos,
            slot,
        );
        assert_eq!(
            clamped, slot,
            "a mover that can climb the cliff must keep the raw slot - it is reachable under its \
             own terrain even though GROUND's view would have clamped it away"
        );
    }

    #[test]
    fn a_standing_horse_holds_inside_one_acceleration_step() {
        let limits = mint_movement::MotionLimits {
            max_speed: 45.0,
            acceleration: 90.0,
            turn_rate: 3.0,
            ..crate::unit::PLACEHOLDER_LIMITS
        };
        let horse = HordeMotion {
            facing: Facing::EAST,
            moving: false,
            slow_movement: false,
            horse: true,
            horse_slow: false,
        };
        let standing = MoverState::new(Vec2::new(-3.0, 0.0), Facing::EAST);
        let mut walking = standing;
        walking.speed = 10.0;
        let acts = (
            hub(standing, Vec2::ZERO, horse, &limits, 1.0 / 30.0),
            matches!(
                hub(walking, Vec2::ZERO, horse, &limits, 1.0 / 30.0),
                Act::Step { .. }
            ),
        );
        assert_eq!(acts, (Act::Settle(None), true));
    }

    #[test]
    fn the_hub_picks_each_arm_by_retail_thresholds() {
        let limits = mint_movement::MotionLimits {
            max_speed: 45.0,
            acceleration: 45.0,
            turn_rate: 3.0,
            ..crate::unit::PLACEHOLDER_LIMITS
        };
        let dt = 1.0 / 30.0;
        let standing = HordeMotion {
            facing: Facing::EAST,
            moving: false,
            slow_movement: false,
            horse: false,
            horse_slow: false,
        };
        let marching = HordeMotion {
            moving: true,
            ..standing
        };
        let at = |x: f32, radians: f32| {
            MoverState::new(Vec2::new(x, 0.0), Facing::from_radians(radians))
        };
        let slot = Vec2::ZERO;
        let step = 45.0 * dt;
        let rows: [(&str, MoverState, HordeMotion, Act); 9] = [
            (
                "far: a full step at the slot",
                at(-10.0, 0.0),
                standing,
                Act::Step { goal: slot, step },
            ),
            (
                "just past the near threshold, 0.2 retail steps over a retail frame",
                at(-1.81, 0.0),
                standing,
                Act::Step { goal: slot, step },
            ),
            (
                "near and facing the horde: settled onto the slot",
                at(-1.79, 0.0),
                standing,
                Act::Settle(Some(slot)),
            ),
            (
                "near but 11 degrees off: turns in place",
                at(0.0, 0.192),
                standing,
                Act::Snap(Facing::EAST),
            ),
            (
                "near while the horde moves: stands, not settled",
                at(0.0, 0.0),
                marching,
                Act::Stand,
            ),
            (
                "1 behind a moving slot: past a tick's share of the frame's distance, a step",
                at(-1.0, 0.0),
                marching,
                Act::Step { goal: slot, step },
            ),
            (
                "more than 4 retail steps behind a moving horde: 1.5 times",
                at(-37.0, 0.0),
                marching,
                Act::Step {
                    goal: slot,
                    step: 45.0 * 1.5 * dt,
                },
            ),
            (
                "ahead of its slot with UseSlowHordeMovement: stands",
                at(10.0, 0.0),
                HordeMotion {
                    slow_movement: true,
                    ..marching
                },
                Act::Stand,
            ),
            (
                "a horse closes at its gap per retail frame",
                at(-2.0, 0.0),
                HordeMotion {
                    horse: true,
                    ..standing
                },
                Act::Step {
                    goal: slot,
                    step: 2.0 / RETAIL_FRAME_SECONDS * dt,
                },
            ),
        ];
        for (rule, state, horde, expected) in rows {
            assert_eq!(hub(state, slot, horde, &limits, dt), expected, "{rule}");
        }
    }

    #[test]
    fn slot_poses_rotate_with_the_horde() {
        let horde = Horde {
            formation: 0,
            slots: slots_at(&[Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)]),
            members: Vec::new(),
            free: Vec::new(),
            dirty: false,
            banner_slot: None,
            centre: Vec2::ZERO,
            climb_wall: None,
            climb_quiet: 0,
        };
        let east = slot_poses(
            &horde,
            &MoverState::new(Vec2::new(100.0, 100.0), Facing::EAST),
        );
        assert!((east[0].x - 110.0).abs() < 1e-4 && (east[0].y - 100.0).abs() < 1e-4);
        assert!((east[1].x - 100.0).abs() < 1e-4 && (east[1].y - 110.0).abs() < 1e-4);

        let north = slot_poses(
            &horde,
            &MoverState::new(
                Vec2::new(100.0, 100.0),
                Facing::from_radians(core::f32::consts::FRAC_PI_2),
            ),
        );
        assert!((north[0].x - 100.0).abs() < 1e-3 && (north[0].y - 110.0).abs() < 1e-3);
        assert!((north[1].x - 90.0).abs() < 1e-3 && (north[1].y - 100.0).abs() < 1e-3);
    }
}
