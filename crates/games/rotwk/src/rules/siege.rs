use mint_movement::{Facing, Route, Vec2};
use mint_sage_modules::{DeployStyleDef, Effect, PieceShape, SiegeDeployDef};
use mint_sim::{Component, Entity, Frame, LogicFrames, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::AttackOrder;
use crate::unit::{Path, PathKind};

pub(crate) const MAX_SLOTS: usize = 10;

const DOCK_ANGLE: f32 = 0.15;

const WALL_STEP: f32 = 5.0;
const WALL_STEPS: u32 = 10;

const OFFSET_SCALE: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DockBone {
    pub(crate) ladder: bool,
    pub(crate) at: Vec2,
    pub(crate) axis: Facing,
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    ladder: bool,
    at: Vec2,
    dir: Vec2,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DockOccupants(Vec<Option<Entity>>);

impl Component for DockOccupants {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for held in &self.0 {
            hasher.write_u32(held.map_or(u32::MAX, Entity::index));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SiegePhase {
    Idle,
    Approach,
    Lowering,
    Deployed,
    Raising,
}

impl SiegePhase {
    const fn code(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Approach => 1,
            Self::Lowering => 2,
            Self::Deployed => 3,
            Self::Raising => 4,
        }
    }

    const fn active(self) -> bool {
        matches!(self, Self::Lowering | Self::Deployed | Self::Raising)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SiegeDeploy {
    phase: SiegePhase,
    counter: u32,
    wall: Option<Entity>,
    dock: Vec2,
    dir: Vec2,
    toggle: bool,
    pending_stop: bool,
    busy_until: Frame,
}

impl Component for SiegeDeploy {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.phase.code());
        hasher.write_u32(self.counter);
        hasher.write_u32(self.wall.map_or(u32::MAX, Entity::index));
        for value in [self.dock.x, self.dock.y, self.dir.x, self.dir.y] {
            hasher.write_f32(value);
        }
        hasher.write_bool(self.toggle);
        hasher.write_bool(self.pending_stop);
        hasher.write_u64(self.busy_until.get());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeployState {
    ReadyToMove,
    Deploy,
    ReadyToAttack,
    Undeploy,
}

impl DeployState {
    const fn code(self) -> u8 {
        match self {
            Self::ReadyToMove => 0,
            Self::Deploy => 1,
            Self::ReadyToAttack => 2,
            Self::Undeploy => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DeployStyle {
    state: DeployState,
    timer: Frame,
    deploy_request: bool,
    undeploy_request: bool,
    deferred_route: Option<Path>,
    deferred_attack: Option<AttackOrder>,
    modifier_applied: bool,
}

impl Default for DeployStyle {
    fn default() -> Self {
        Self {
            state: DeployState::ReadyToMove,
            timer: Frame::ZERO,
            deploy_request: false,
            undeploy_request: false,
            deferred_route: None,
            deferred_attack: None,
            modifier_applied: false,
        }
    }
}

impl Component for DeployStyle {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.state.code());
        hasher.write_u64(self.timer.get());
        hasher.write_bool(self.deploy_request);
        hasher.write_bool(self.undeploy_request);
        hasher.write_bool(self.deferred_route.is_some());
        if let Some(path) = &self.deferred_route {
            path.hash_into(hasher);
        }
        hasher.write_bool(self.deferred_attack.is_some());
        if let Some(order) = &self.deferred_attack {
            order.hash_into(hasher);
        }
        hasher.write_bool(self.modifier_applied);
    }
}

fn ticks(ms: f32) -> LogicFrames {
    if !ms.is_finite() || ms <= 0.0 {
        return LogicFrames::ZERO;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = ms.min(3_600_000.0) as u32;
    LogicFrames::from_milliseconds(whole)
}

fn placed(origin: Vec2, facing: Facing, local: Vec2) -> Vec2 {
    let (sin, cos) = mint_math::f32::sin_cos(facing.radians());
    Vec2::new(
        origin.x + local.x * cos - local.y * sin,
        origin.y + local.x * sin + local.y * cos,
    )
}

fn normalized(v: Vec2) -> Option<Vec2> {
    let length = v.length();
    (length.is_finite() && length > 0.0).then(|| Vec2::new(v.x / length, v.y / length))
}

impl RotwkRules {
    fn siege_deploy_def(&self, world: &World, ids: Ids, entity: Entity) -> Option<&SiegeDeployDef> {
        let kind = world.get(ids.kind, entity)?;
        self.kinds.get(*kind)?.record.siege.deploy.as_ref()
    }

    fn deploy_style_def(&self, world: &World, ids: Ids, entity: Entity) -> Option<&DeployStyleDef> {
        let kind = world.get(ids.kind, entity)?;
        self.kinds.get(*kind)?.record.siege.deploy_style.as_ref()
    }

    fn docks(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.siege.docking)
    }

    pub(super) fn siege_refuses_order(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        let gated = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.siege.gated);
        gated
            && world
                .get(ids.siege_deploy, entity)
                .is_some_and(|deploy| deploy.phase.active())
    }

    pub(super) fn siege_deployed(world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .get(ids.siege_deploy, entity)
            .is_some_and(|deploy| deploy.phase == SiegePhase::Deployed)
    }

    fn slots_of(&self, world: &World, ids: Ids, wall: Entity) -> Vec<Slot> {
        let Some(kind) = world.get(ids.kind, wall) else {
            return Vec::new();
        };
        let Some(body) = world.get(ids.body, wall) else {
            return Vec::new();
        };
        let Some(bones) = self.siege_docks.get(usize::from(kind.0)) else {
            return Vec::new();
        };
        bones
            .iter()
            .map(|bone| {
                let at = placed(body.0.position, body.0.facing, bone.at);
                let axis = Facing::from_radians(bone.axis.radians() + body.0.facing.radians());
                let x = axis.to_vector();
                Slot {
                    ladder: bone.ladder,
                    at,
                    dir: Vec2::new(-x.x, -x.y),
                }
            })
            .collect()
    }

    fn may_use(&self, world: &World, ids: Ids, engine: Entity, slot: &Slot) -> bool {
        !slot.ladder
            || world
                .get(ids.kind, engine)
                .and_then(|kind| self.kinds.get(*kind))
                .is_some_and(|entry| {
                    entry
                        .record
                        .kind_of
                        .iter()
                        .any(|kind| kind.eq_ignore_ascii_case("SIEGE_LADDER"))
                })
    }

    fn live_occupants(&self, world: &World, ids: Ids, wall: Entity, count: usize) -> DockOccupants {
        let mut held = world
            .get(ids.dock_occupants, wall)
            .cloned()
            .unwrap_or_default();
        held.0.resize(count, None);
        for slot in &mut held.0 {
            let stale = slot.is_some_and(|occupant| {
                self.is_gone_object(world, ids, occupant)
                    || world
                        .get(ids.siege_deploy, occupant)
                        .is_none_or(|deploy| deploy.wall != Some(wall))
            });
            if stale {
                *slot = None;
            }
        }
        held
    }

    fn is_gone_object(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        !world.contains(ids.body, entity)
            || world.contains(ids.doomed, entity)
            || world
                .get(ids.health, entity)
                .is_some_and(|health| health.0.is_dead())
    }

    fn can_dock(&self, world: &World, ids: Ids, wall: Entity, engine: Entity) -> bool {
        let slots = self.slots_of(world, ids, wall);
        let held = self.live_occupants(world, ids, wall, slots.len());
        slots
            .iter()
            .zip(&held.0)
            .any(|(slot, occupant)| occupant.is_none() && self.may_use(world, ids, engine, slot))
    }

    fn reserve(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wall: Entity,
        engine: Entity,
    ) -> Result<Option<Slot>> {
        let world = tick.state();
        let Some(from) = world.get(ids.body, engine).map(|body| body.0.position) else {
            return Ok(None);
        };
        let slots = self.slots_of(world, ids, wall);
        let mut held = self.live_occupants(world, ids, wall, slots.len());
        if let Some(own) = held
            .0
            .iter_mut()
            .find(|occupant| **occupant == Some(engine))
        {
            *own = None;
        }
        let best = slots
            .iter()
            .enumerate()
            .filter(|(index, slot)| {
                held.0.get(*index).is_some_and(Option::is_none)
                    && self.may_use(world, ids, engine, slot)
            })
            .min_by(|(_, a), (_, b)| {
                a.at.minus(from)
                    .length()
                    .total_cmp(&b.at.minus(from).length())
            })
            .map(|(index, slot)| (index, *slot));
        let Some((index, slot)) = best else {
            tick.state_mut().insert(ids.dock_occupants, wall, held)?;
            return Ok(None);
        };
        if let Some(entry) = held.0.get_mut(index) {
            *entry = Some(engine);
        }
        tick.state_mut().insert(ids.dock_occupants, wall, held)?;
        Ok(Some(slot))
    }

    fn point_on_wall(&self, world: &World, ids: Ids, point: Vec2, margin: f32) -> bool {
        let Some(store) = world.store(ids.kind) else {
            return false;
        };
        store.iter().any(|(entity, kind)| {
            let Some(entry) = self.kinds.get(*kind) else {
                return false;
            };
            if !entry
                .record
                .kind_of
                .iter()
                .any(|flag| flag.eq_ignore_ascii_case("WALK_ON_TOP_OF_WALL"))
            {
                return false;
            }
            let Some(body) = world.get(ids.body, entity) else {
                return false;
            };
            entry.record.geometry.shown(&[]).any(|piece| {
                let centre = placed(
                    body.0.position,
                    body.0.facing,
                    Vec2::new(piece.offset.0, piece.offset.1),
                );
                let local = point.minus(centre);
                let (sin, cos) = mint_math::f32::sin_cos(body.0.facing.radians());
                let (x, y) = (
                    local.x * cos + local.y * sin,
                    -local.x * sin + local.y * cos,
                );
                match piece.shape {
                    PieceShape::Box { major, minor } => {
                        x.abs() <= major + margin && y.abs() <= minor + margin
                    }
                    PieceShape::Disc { radius } => {
                        x * x + y * y <= (radius + margin) * (radius + margin)
                    }
                }
            })
        })
    }

    fn approach_point(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: Entity,
        wall: Entity,
        def: &SiegeDeployDef,
    ) -> Result<Option<(Vec2, Vec2)>> {
        let Some(slot) = self.reserve(tick, ids, wall, engine)? else {
            return Ok(None);
        };
        let world = tick.state();
        let Some(wall_at) = world.get(ids.body, wall).map(|body| body.0.position) else {
            return Ok(None);
        };
        let mut point = slot.at;
        let offset = point.minus(wall_at);
        if offset.x * offset.x + offset.y * offset.y > 1.0 {
            let scaled = Vec2::new(offset.x * OFFSET_SCALE, offset.y * OFFSET_SCALE);
            if !def.skip_adjust_position {
                point = Vec2::new(wall_at.x + scaled.x, wall_at.y + scaled.y);
            }
            if let Some(n) = normalized(scaled) {
                for _ in 0..WALL_STEPS {
                    if !self.point_on_wall(world, ids, point, 0.0) {
                        break;
                    }
                    point = Vec2::new(point.x + n.x * WALL_STEP, point.y + n.y * WALL_STEP);
                }
                point = Vec2::new(
                    point.x + n.x * def.extra_wall_distance,
                    point.y + n.y * def.extra_wall_distance,
                );
            }
        }
        Ok(Some((point, slot.dir)))
    }

    pub(super) fn initiate_siege_deploy(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: Entity,
        target: Entity,
    ) -> Result<()> {
        let Some(def) = self.siege_deploy_def(tick.state(), ids, engine).cloned() else {
            return Ok(());
        };
        let world = tick.state();
        let Some(target_body) = world.get(ids.body, target).copied() else {
            return Ok(());
        };
        if !self.docks(world, ids, target) {
            return Ok(());
        }
        let wall = if self.can_dock(world, ids, target, engine) {
            Some(target)
        } else {
            let reach = def.wall_search_distance * def.wall_search_distance;
            world.store(ids.body).and_then(|store| {
                store.iter().find_map(|(candidate, body)| {
                    let gap = body.0.position.minus(target_body.0.position);
                    (candidate != target
                        && gap.x * gap.x + gap.y * gap.y <= reach
                        && self.docks(world, ids, candidate)
                        && self.can_dock(world, ids, candidate, engine))
                    .then_some(candidate)
                })
            })
        };
        tick.state_mut().remove(ids.player_order, engine);
        let mut deploy = tick
            .state()
            .get(ids.siege_deploy, engine)
            .cloned()
            .unwrap_or(SiegeDeploy {
                phase: SiegePhase::Idle,
                counter: 0,
                wall: None,
                dock: Vec2::ZERO,
                dir: Vec2::ZERO,
                toggle: false,
                pending_stop: false,
                busy_until: Frame::ZERO,
            });
        match wall {
            Some(wall) => match self.approach_point(tick, ids, engine, wall, &def)? {
                Some((dock, dir)) => {
                    deploy.dock = dock;
                    deploy.dir = dir;
                    deploy.toggle = true;
                    deploy.wall = Some(wall);
                    deploy.phase = SiegePhase::Approach;
                    self.ignore_wall(tick, ids, engine, wall)?;
                    walk(tick, ids, engine, dock)?;
                }
                None => {
                    deploy.wall = None;
                    deploy.phase = SiegePhase::Idle;
                }
            },
            None => {
                let to = if def.away_from_wall_wait_dist > 0.0 {
                    let x = target_body.0.facing.to_vector();
                    Vec2::new(
                        target_body.0.position.x + x.x * def.away_from_wall_wait_dist,
                        target_body.0.position.y + x.y * def.away_from_wall_wait_dist,
                    )
                } else {
                    target_body.0.position
                };
                walk(tick, ids, engine, to)?;
            }
        }
        crate::diag::diag!(
            "siege: entity {} deploys at {:?}, phase {:?}",
            engine.index(),
            deploy.wall.map(Entity::index),
            deploy.phase
        );
        tick.state_mut().insert(ids.siege_deploy, engine, deploy)?;
        Ok(())
    }

    fn ignore_wall(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: Entity,
        wall: Entity,
    ) -> Result<()> {
        tick.state_mut().insert(
            ids.ignored_obstacle,
            engine,
            super::movement::IgnoredObstacle(wall),
        )?;
        Ok(())
    }

    pub(super) fn stop_siege_deploy(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: Entity,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let Some(mut deploy) = tick.state().get(ids.siege_deploy, engine).cloned() else {
            return Ok(());
        };
        match deploy.phase {
            SiegePhase::Idle | SiegePhase::Approach => {
                self.set_siege_phase(tick, ids, engine, &mut deploy, SiegePhase::Idle, effects)?;
            }
            SiegePhase::Lowering | SiegePhase::Deployed => deploy.pending_stop = true,
            SiegePhase::Raising => {}
        }
        tick.state_mut().insert(ids.siege_deploy, engine, deploy)?;
        Ok(())
    }

    pub(super) fn step_siege(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let engines: Vec<Entity> = tick
            .state()
            .store(ids.siege_deploy)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let mut effects = Vec::new();
        for engine in engines {
            self.update_siege(tick, ids, engine, &mut effects)?;
        }
        self.step_deploy_style(tick, ids, &mut effects)?;
        self.apply_effects(tick, ids, effects)
    }

    fn update_siege(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: Entity,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let Some(mut deploy) = tick.state().get(ids.siege_deploy, engine).cloned() else {
            return Ok(());
        };
        let Some(def) = self.siege_deploy_def(tick.state(), ids, engine).cloned() else {
            return Ok(());
        };
        let now = tick.frame();
        if self.is_gone_object(tick.state(), ids, engine) || deploy.phase == SiegePhase::Idle {
            return Ok(());
        }
        if deploy.pending_stop {
            deploy.pending_stop = false;
            self.set_siege_phase(tick, ids, engine, &mut deploy, SiegePhase::Raising, effects)?;
            tick.state_mut().insert(ids.siege_deploy, engine, deploy)?;
            return Ok(());
        }
        let idle = !tick.state().contains(ids.path, engine);
        if tick.state().contains(ids.player_order, engine) {
            let _ = idle;
            self.set_siege_phase(tick, ids, engine, &mut deploy, SiegePhase::Idle, effects)?;
            tick.state_mut().insert(ids.siege_deploy, engine, deploy)?;
            return Ok(());
        }
        let wall = deploy
            .wall
            .filter(|wall| !self.is_gone_object(tick.state(), ids, *wall));
        if wall.is_none() {
            deploy.wall = None;
            if deploy.phase != SiegePhase::Raising {
                self.set_siege_phase(tick, ids, engine, &mut deploy, SiegePhase::Raising, effects)?;
                tick.state_mut().insert(ids.siege_deploy, engine, deploy)?;
                return Ok(());
            }
        }
        match deploy.phase {
            SiegePhase::Approach => {
                let Some(wall) = wall else {
                    return Ok(());
                };
                let world = tick.state();
                let Some(body) = world.get(ids.body, engine).copied() else {
                    return Ok(());
                };
                let aligned = Facing::from_vector(deploy.dir)
                    .is_some_and(|wanted| body.0.facing.delta_to(wanted).abs() < DOCK_ANGLE);
                let radius = world
                    .get(ids.shape, engine)
                    .map_or(0.0, |shape| shape.radius);
                let touching = self.point_on_wall(world, ids, body.0.position, radius);
                if aligned && (touching || idle) && self.end_portal_above_wall(world, ids, engine) {
                    tick.state_mut().remove(ids.path, engine);
                    self.set_siege_phase(
                        tick,
                        ids,
                        engine,
                        &mut deploy,
                        SiegePhase::Lowering,
                        effects,
                    )?;
                } else if idle {
                    self.ignore_wall(tick, ids, engine, wall)?;
                    deploy.toggle = !deploy.toggle;
                    if deploy.toggle {
                        if let Some((dock, dir)) =
                            self.approach_point(tick, ids, engine, wall, &def)?
                        {
                            deploy.dock = dock;
                            deploy.dir = dir;
                            walk(tick, ids, engine, dock)?;
                        }
                    } else {
                        let towards = Vec2::new(
                            body.0.position.x + deploy.dir.x,
                            body.0.position.y + deploy.dir.y,
                        );
                        self.turn_towards(tick, ids, engine, towards)?;
                    }
                }
            }
            SiegePhase::Lowering => {
                deploy.counter = deploy.counter.saturating_add(1);
                if deploy.counter >= ticks(def.lower_delay_ms).get() {
                    self.set_siege_phase(
                        tick,
                        ids,
                        engine,
                        &mut deploy,
                        SiegePhase::Deployed,
                        effects,
                    )?;
                }
            }
            SiegePhase::Deployed | SiegePhase::Idle => {}
            SiegePhase::Raising => {
                if now >= deploy.busy_until {
                    self.set_siege_phase(
                        tick,
                        ids,
                        engine,
                        &mut deploy,
                        SiegePhase::Idle,
                        effects,
                    )?;
                }
            }
        }
        tick.state_mut().insert(ids.siege_deploy, engine, deploy)?;
        Ok(())
    }

    fn end_portal_above_wall(&self, world: &World, ids: Ids, engine: Entity) -> bool {
        let Some(kind) = world.get(ids.kind, engine) else {
            return false;
        };
        let Some(Some(bone)) = self.siege_above_wall.get(usize::from(kind.0)) else {
            return false;
        };
        let Some(body) = world.get(ids.body, engine) else {
            return false;
        };
        let at = placed(body.0.position, body.0.facing, *bone);
        self.point_on_wall(world, ids, at, 0.0)
    }

    fn set_siege_phase(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: Entity,
        deploy: &mut SiegeDeploy,
        phase: SiegePhase,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let conditions =
            |effects: &mut Vec<(Entity, Effect)>, target: Entity, add: &[&str], remove: &[&str]| {
                effects.push((
                    target,
                    Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(
                            add.iter().map(|name| (*name).to_owned()).collect(),
                        ),
                        remove: mint_sage_modules::effects::names(
                            remove.iter().map(|name| (*name).to_owned()).collect(),
                        ),
                    },
                ));
            };
        let passengers = tick
            .state()
            .get(ids.passengers, engine)
            .map(|list| list.0.clone())
            .unwrap_or_default();
        match deploy.phase {
            SiegePhase::Lowering => {
                conditions(effects, engine, &[], &["UNPACKING"]);
                for passenger in &passengers {
                    conditions(effects, *passenger, &[], &["UNPACKING"]);
                }
            }
            SiegePhase::Deployed => conditions(effects, engine, &[], &["DEPLOYED"]),
            SiegePhase::Raising => {
                conditions(effects, engine, &[], &["PACKING"]);
                deploy.wall = None;
            }
            SiegePhase::Idle | SiegePhase::Approach => {}
        }
        deploy.phase = phase;
        deploy.counter = 0;
        match phase {
            SiegePhase::Lowering => {
                conditions(effects, engine, &["UNPACKING"], &[]);
                for passenger in &passengers {
                    conditions(effects, *passenger, &["UNPACKING"], &[]);
                }
                if let Some(mut body) = tick.state().get(ids.body, engine).copied() {
                    body.0.speed = 0.0;
                    tick.state_mut().insert(ids.body, engine, body)?;
                }
            }
            SiegePhase::Deployed => {
                conditions(effects, engine, &["DEPLOYED"], &["MOVING"]);
                if self
                    .siege_deploy_def(tick.state(), ids, engine)
                    .is_some_and(|def| def.evacuate_passengers_on_deploy)
                {
                    self.evacuate(tick, ids, engine)?;
                }
            }
            SiegePhase::Raising => {
                conditions(effects, engine, &["PACKING"], &[]);
                let raise = self
                    .siege_deploy_def(tick.state(), ids, engine)
                    .map_or(LogicFrames::ZERO, |def| ticks(def.raise_delay_ms));
                deploy.busy_until = tick.frame().checked_add(raise).unwrap_or(Frame::FOREVER);
            }
            SiegePhase::Idle => {
                deploy.wall = None;
                tick.state_mut().remove(ids.ignored_obstacle, engine);
            }
            SiegePhase::Approach => {}
        }
        crate::diag::diag!("siege: entity {} phase {:?}", engine.index(), phase);
        Ok(())
    }

    fn step_deploy_style(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let now = tick.frame();
        let engines: Vec<Entity> = tick
            .state()
            .store(ids.kind)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, kind)| {
                        self.kinds
                            .get(**kind)
                            .is_some_and(|entry| entry.record.siege.deploy_style.is_some())
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for engine in engines {
            if self.is_gone_object(tick.state(), ids, engine) {
                continue;
            }
            let Some(def) = self.deploy_style_def(tick.state(), ids, engine).cloned() else {
                continue;
            };
            let mut machine = tick
                .state()
                .get(ids.deploy_style, engine)
                .cloned()
                .unwrap_or_default();
            let target = tick
                .state()
                .get(ids.attack_order, engine)
                .map(|order| order.target);
            let in_range =
                target.is_some_and(|target| self.weapon_reaches(tick.state(), ids, engine, target));
            let moving = tick.state().contains(ids.path, engine);
            match machine.state {
                DeployState::ReadyToMove => {
                    if machine.deploy_request || (def.must_deploy_to_attack && in_range) {
                        machine.deploy_request = false;
                        self.enter_deploy_state(
                            tick,
                            ids,
                            engine,
                            &mut machine,
                            &def,
                            DeployState::Deploy,
                            effects,
                        )?;
                    }
                }
                DeployState::Deploy | DeployState::Undeploy => {
                    if let Some(path) = tick.state_mut().remove(ids.path, engine) {
                        machine.deferred_route = Some(path);
                    }
                    if let Some(order) = tick.state_mut().remove(ids.attack_order, engine) {
                        machine.deferred_attack = Some(order);
                    }
                    if now >= machine.timer {
                        let next = if machine.state == DeployState::Deploy {
                            DeployState::ReadyToAttack
                        } else {
                            DeployState::ReadyToMove
                        };
                        self.enter_deploy_state(
                            tick,
                            ids,
                            engine,
                            &mut machine,
                            &def,
                            next,
                            effects,
                        )?;
                    }
                }
                DeployState::ReadyToAttack => {
                    let out_of_range = target.is_some() && !in_range;
                    if out_of_range || moving || machine.undeploy_request {
                        if machine.undeploy_request && def.must_deploy_to_attack {
                            tick.state_mut().remove(ids.attack_order, engine);
                        }
                        machine.undeploy_request = false;
                        self.enter_deploy_state(
                            tick,
                            ids,
                            engine,
                            &mut machine,
                            &def,
                            DeployState::Undeploy,
                            effects,
                        )?;
                    }
                }
            }
            tick.state_mut().insert(ids.deploy_style, engine, machine)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn enter_deploy_state(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: Entity,
        machine: &mut DeployStyle,
        def: &DeployStyleDef,
        state: DeployState,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let now = tick.frame();
        let mut conditions = |add: &[&str], remove: &[&str]| {
            effects.push((
                engine,
                Effect::ModelConditions {
                    add: mint_sage_modules::effects::names(
                        add.iter().map(|name| (*name).to_owned()).collect(),
                    ),
                    remove: mint_sage_modules::effects::names(
                        remove.iter().map(|name| (*name).to_owned()).collect(),
                    ),
                },
            ));
        };
        machine.state = state;
        match state {
            DeployState::Deploy => {
                if let Some(path) = tick.state_mut().remove(ids.path, engine) {
                    machine.deferred_route = Some(path);
                }
                if let Some(order) = tick.state_mut().remove(ids.attack_order, engine) {
                    machine.deferred_attack = Some(order);
                }
                conditions(&["UNPACKING"], &["PACKING"]);
                machine.timer = now
                    .checked_add(ticks(def.unpack_time_ms))
                    .unwrap_or(Frame::FOREVER);
            }
            DeployState::ReadyToAttack => {
                machine.timer = Frame::ZERO;
                conditions(&["DEPLOYED"], &["UNPACKING"]);
                if let Some(list) = &def.deployed_attribute_modifier {
                    if let Some(id) = self.combat.modifiers.id(list) {
                        let mut timed = tick
                            .state()
                            .get(ids.timed_modifiers, engine)
                            .cloned()
                            .unwrap_or_default();
                        timed.give(id, Frame::FOREVER);
                        tick.state_mut()
                            .insert(ids.timed_modifiers, engine, timed)?;
                        self.recompute_bonuses(tick, ids, engine)?;
                        machine.modifier_applied = true;
                    }
                }
                self.replay_deferred(tick, ids, engine, machine)?;
            }
            DeployState::Undeploy => {
                if let Some(path) = tick.state_mut().remove(ids.path, engine) {
                    machine.deferred_route = Some(path);
                }
                conditions(&["PACKING"], &["UNPACKING", "DEPLOYED"]);
                machine.timer = now
                    .checked_add(ticks(def.pack_time_ms))
                    .unwrap_or(Frame::FOREVER);
                if machine.modifier_applied {
                    if let Some(id) = def
                        .deployed_attribute_modifier
                        .as_deref()
                        .and_then(|list| self.combat.modifiers.id(list))
                    {
                        if let Some(timed) = tick.state_mut().get_mut(ids.timed_modifiers, engine) {
                            timed.0.retain(|(held, _)| *held != id);
                        }
                        self.recompute_bonuses(tick, ids, engine)?;
                    }
                    machine.modifier_applied = false;
                }
            }
            DeployState::ReadyToMove => {
                machine.timer = Frame::ZERO;
                conditions(&[], &["PACKING"]);
                self.replay_deferred(tick, ids, engine, machine)?;
            }
        }
        crate::diag::diag!("deploy: entity {} -> {:?}", engine.index(), state);
        Ok(())
    }

    fn replay_deferred(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: Entity,
        machine: &mut DeployStyle,
    ) -> Result<()> {
        if let Some(order) = machine.deferred_attack.take() {
            if !self.is_gone_object(tick.state(), ids, order.target) {
                tick.state_mut().insert(ids.attack_order, engine, order)?;
            }
        }
        if let Some(path) = machine.deferred_route.take() {
            tick.state_mut().insert(ids.path, engine, path)?;
        }
        Ok(())
    }

    fn weapon_reaches(&self, world: &World, ids: Ids, engine: Entity, target: Entity) -> bool {
        let (Some(armed), Some(from), Some(to)) = (
            world.get(ids.armed, engine),
            world.get(ids.body, engine),
            world.get(ids.body, target),
        ) else {
            return false;
        };
        let Some(weapon) = self.combat.weapons.get(armed.weapon) else {
            return false;
        };
        let radius = |entity: Entity| {
            world
                .get(ids.shape, entity)
                .map_or(0.0, |shape| shape.radius)
        };
        crate::combat::in_range(
            from.0.position,
            radius(engine),
            to.0.position,
            radius(target),
            weapon.minimum_attack_range,
            weapon.attack_range,
        )
    }

    pub(super) fn deploy_transition(world: &World, ids: Ids, entity: Entity) -> bool {
        world.get(ids.deploy_style, entity).is_some_and(|machine| {
            matches!(machine.state, DeployState::Deploy | DeployState::Undeploy)
        })
    }

    pub(super) fn deploy_holds_fire(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        let Some(def) = self.deploy_style_def(world, ids, entity) else {
            return false;
        };
        let state = world
            .get(ids.deploy_style, entity)
            .map_or(DeployState::ReadyToMove, |machine| machine.state);
        matches!(state, DeployState::Deploy | DeployState::Undeploy)
            || (def.must_deploy_to_attack && state != DeployState::ReadyToAttack)
    }

    pub(super) fn toggle_deploy(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: Entity,
    ) -> Result<()> {
        let mut machine = tick
            .state()
            .get(ids.deploy_style, engine)
            .cloned()
            .unwrap_or_default();
        let sound = tick
            .state()
            .get(ids.kind, engine)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.siege.toggle_deploy.as_ref());
        let deployed = machine.state == DeployState::ReadyToAttack;
        if deployed {
            machine.deploy_request = false;
            machine.undeploy_request = !matches!(
                machine.state,
                DeployState::ReadyToMove | DeployState::Undeploy
            );
            tick.state_mut().insert(ids.deploy_style, engine, machine)?;
            if let Some(name) = sound.and_then(|def| def.sound_undeploy.as_deref()) {
                self.emit_object_sound(tick, ids, engine, std::sync::Arc::from(name));
            }
        } else {
            if matches!(machine.state, DeployState::Deploy | DeployState::Undeploy) {
                return Ok(());
            }
            if let Some(path) = tick.state_mut().remove(ids.path, engine) {
                let _ = path;
            }
            machine.undeploy_request = false;
            machine.deploy_request = !matches!(
                machine.state,
                DeployState::Deploy | DeployState::ReadyToAttack
            );
            tick.state_mut().insert(ids.deploy_style, engine, machine)?;
            if let Some(name) = sound.and_then(|def| def.sound_deploy.as_deref()) {
                self.emit_object_sound(tick, ids, engine, std::sync::Arc::from(name));
            }
        }
        Ok(())
    }

    pub(super) fn siege_power(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power: &str,
        target: Option<Entity>,
    ) -> Result<bool> {
        let Some(record) = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record)
        else {
            return Ok(false);
        };
        let siege = record.siege.clone();
        if siege
            .deploy
            .as_ref()
            .is_some_and(|def| def.power.eq_ignore_ascii_case(power))
        {
            if let Some(target) = target {
                self.initiate_siege_deploy(tick, ids, source, target)?;
            }
            return Ok(true);
        }
        if let Some((_, stopped)) = siege
            .stop_powers
            .iter()
            .find(|(own, _)| own.eq_ignore_ascii_case(power))
        {
            if siege
                .deploy
                .as_ref()
                .is_some_and(|def| def.power.eq_ignore_ascii_case(stopped))
            {
                let mut effects = Vec::new();
                self.stop_siege_deploy(tick, ids, source, &mut effects)?;
                self.apply_effects(tick, ids, effects)?;
            }
            return Ok(true);
        }
        if siege
            .toggle_deploy
            .as_ref()
            .is_some_and(|def| def.power.eq_ignore_ascii_case(power))
        {
            self.toggle_deploy(tick, ids, source)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn evacuate(&self, tick: &mut Tick<'_>, ids: Ids, container: Entity) -> Result<()> {
        let passengers = tick
            .state()
            .get(ids.passengers, container)
            .map(|list| list.0.clone())
            .unwrap_or_default();
        let now = tick.frame();
        for passenger in passengers {
            if let Some(held) = tick.state_mut().get_mut(ids.contained, passenger) {
                held.leaves_at = now;
            }
        }
        Ok(())
    }
}

fn walk(tick: &mut Tick<'_>, ids: Ids, entity: Entity, to: Vec2) -> Result<()> {
    crate::unit::give_path(
        tick.state_mut(),
        (ids.body, ids.path),
        entity,
        Path {
            route: Route::to_point(to),
            goal: to,
            kind: PathKind::Ability,
            facing: None,
        },
    )?;
    Ok(())
}
