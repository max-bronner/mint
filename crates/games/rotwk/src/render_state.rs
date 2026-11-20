use mint_movement::{Facing, Vec2};
use mint_render::Vec3;
use mint_sage_scene::Conditions;
use mint_sim::{Frame, LogicFrames, PlayerId, World};

use crate::rules::{Ids, MatchClock};
pub(crate) use crate::weapon_use::{weapon_phase, ArmedView, Attack};
use mint_sage_asset::{KindOf, PickType};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UnitView {
    pub(crate) entity: mint_sim::Entity,
    pub(crate) position: Vec2,
    pub(crate) ground: f32,
    pub(crate) facing: f32,
    pub(crate) radius: f32,
    pub(crate) height: f32,
    pub(crate) kind_of: KindOf,
    pub(crate) targetable: bool,
    pub(crate) pick_type: PickType,
    pub(crate) owner: PlayerId,
    pub(crate) selected: bool,
    pub(crate) moving: bool,
    pub(crate) health: Option<f32>,
    pub(crate) dying: bool,
    pub(crate) created: u64,
    pub(crate) speed: f32,
    pub(crate) weapon_cycle: f32,
    pub(crate) visibility: mint_sage_shroud::ObjectVisibility,
    pub(crate) kind: Option<u16>,
    pub(crate) art_kind: Option<u16>,
    pub(crate) told_kind: Option<u16>,
    pub(crate) hover_kind: Option<u16>,
    pub(crate) conditions: Conditions,
    pub(crate) attack_cycle: u32,
    pub(crate) attack_slot: Option<usize>,
    pub(crate) build_elapsed: Option<u32>,
    pub(crate) build_total: Option<u32>,
    pub(crate) under_repair: bool,
    pub(crate) held: bool,
}

const STANDING_STILL: f32 = 0.05;

#[must_use]
pub(crate) fn is_moving(has_route: bool, speed: f32) -> bool {
    has_route || speed.abs() > STANDING_STILL
}

#[allow(clippy::too_many_arguments)]
fn unit_conditions(
    moving: bool,
    selected: bool,
    dying: bool,
    structure: bool,
    engaged: bool,
    attack: Attack,
    weapon: Option<ArmedView>,
    now: Frame,
    phase: mint_movement::MotionPhase,
    death_conditions: Conditions,
) -> Conditions {
    let mut conditions = Conditions::NONE;
    if moving {
        conditions = conditions.union(Conditions::MOVING);
    }
    let high_speed = phase.contains(mint_movement::MotionPhase::HIGH_SPEED);
    if phase.contains(mint_movement::MotionPhase::ACCELERATING) {
        conditions = conditions.union(Conditions::ACCELERATE);
    }
    if phase.contains(mint_movement::MotionPhase::DECELERATING) {
        conditions = conditions.union(Conditions::DECELERATE);
    }
    if phase.contains(mint_movement::MotionPhase::WALKING) {
        conditions = conditions.union(Conditions::WALKING);
    }
    if moving && phase.contains(mint_movement::MotionPhase::BACKING_UP) {
        conditions = conditions.union(Conditions::BACKING_UP);
    }
    if phase.contains(mint_movement::MotionPhase::TURNING_LEFT) {
        conditions = conditions.union(Conditions::TURN_LEFT);
        if high_speed {
            conditions = conditions.union(Conditions::TURN_LEFT_HIGH_SPEED);
        }
    }
    if phase.contains(mint_movement::MotionPhase::TURNING_RIGHT) {
        conditions = conditions.union(Conditions::TURN_RIGHT);
        if high_speed {
            conditions = conditions.union(Conditions::TURN_RIGHT_HIGH_SPEED);
        }
    }
    if selected {
        conditions = conditions.union(Conditions::SELECTED);
    }
    if attack != Attack::None {
        conditions = conditions.union(Conditions::ATTACKING);
    }
    if engaged {
        conditions = conditions.union(Conditions::ENGAGED);
    }
    let tick = Frame::new(now.get().saturating_sub(1));
    if let Some(weapon) = weapon {
        if let Some(phase) = weapon_phase(weapon, attack, tick) {
            conditions = conditions.union(Conditions::weapon(weapon.slot, phase));
        }
    }
    if dying {
        conditions = if structure {
            conditions.on_structure_death()
        } else {
            conditions.on_death()
        };
        conditions = conditions.union(death_conditions);
    }
    conditions
}

pub(crate) fn ability_loop_sounds(
    world: &World,
    ids: Ids,
) -> Vec<(mint_sim::Entity, std::sync::Arc<str>)> {
    world
        .store(ids.ability_loop_sound)
        .map(|store| {
            store
                .iter()
                .map(|(entity, loop_sound)| (entity, std::sync::Arc::clone(&loop_sound.0)))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn apply_weather(units: &mut [UnitView], snowy: bool) {
    if !snowy {
        return;
    }
    for unit in units {
        unit.conditions = unit.conditions.union(Conditions::SNOW);
    }
}

use crate::combat::DamageRung;
pub(crate) use crate::combat::DamageThresholds;

pub(crate) fn apply_damage(
    units: &mut [UnitView],
    thresholds: &dyn Fn(&UnitView) -> DamageThresholds,
) {
    for unit in units {
        if unit.build_elapsed.is_some() || unit.dying {
            continue;
        }
        let Some(fraction) = unit.health.filter(|f| f.is_finite()) else {
            continue;
        };
        let state = match thresholds(unit).rung(fraction) {
            DamageRung::ReallyDamaged | DamageRung::Rubble => Conditions::REALLYDAMAGED,
            DamageRung::Damaged => Conditions::DAMAGED,
            DamageRung::Pristine => Conditions::NONE,
        };
        unit.conditions = unit.conditions.union(state);
    }
}

pub(crate) fn apply_kind_damage(
    units: &mut [UnitView],
    kinds: &crate::templates::KindTable,
    global: DamageThresholds,
) {
    apply_damage(units, &|unit| {
        DamageThresholds::of(
            unit.kind
                .and_then(|kind| kinds.get(crate::unit::Kind(kind)))
                .map(|entry| &entry.record),
            global,
        )
    });
}

pub(crate) fn apply_disguise(
    units: &mut [UnitView],
    world: &World,
    rules: &crate::rules::RotwkRules,
    viewer: PlayerId,
) {
    for unit in units {
        unit.art_kind = rules
            .art_kind(world, viewer, unit.entity)
            .map(|kind| kind.0);
        unit.told_kind = rules
            .told_kind(world, viewer, unit.entity)
            .map(|kind| kind.0);
        unit.hover_kind = rules
            .hover_kind(world, viewer, unit.entity)
            .map(|kind| kind.0);
    }
}

pub(crate) fn build_progress(
    elapsed: u32,
    total: Option<u32>,
    record: &mint_sage_modules::ObjectRecord,
) -> Option<f32> {
    let frames = match total {
        Some(frames) => frames,
        None => {
            crate::construction::price_of(
                record,
                &mint_sage_economy::CostModifiers::default(),
                &mint_sage_economy::TimeModifiers::default(),
            )?
            .1
        }
    };
    #[allow(clippy::cast_precision_loss)]
    let progress = elapsed as f32 / frames.max(1) as f32;
    Some(progress.min(1.0))
}

pub(crate) fn apply_collapse(
    units: &mut [UnitView],
    frame: mint_sim::Frame,
    collapse: &dyn Fn(mint_sim::Entity) -> Option<(mint_sage_modules::CollapseState, f32, f32)>,
    shows_post_collapse: &dyn Fn(&UnitView, Conditions) -> bool,
) {
    for unit in units.iter_mut().filter(|unit| unit.dying) {
        let Some((state, sink, max_shudder)) = collapse(unit.entity) else {
            continue;
        };
        if state == mint_sage_modules::CollapseState::Done {
            let done = unit.conditions.on_collapse_done();
            if shows_post_collapse(unit, done) {
                unit.conditions = done;
            }
        }
        unit.ground -= sink;
        if max_shudder > 0.0 {
            let jitter = collapse_shudder(unit.entity, frame, max_shudder);
            unit.position = unit.position.plus(jitter);
        }
    }
}

fn collapse_shudder(entity: mint_sim::Entity, frame: Frame, max_shudder: f32) -> Vec2 {
    let seed = u64::from(entity.index())
        ^ u64::from(entity.generation()).wrapping_shl(32)
        ^ frame.get().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let x = splitmix64(seed);
    let y = splitmix64(x);
    Vec2::new(signed_unit(x) * max_shudder, signed_unit(y) * max_shudder)
}

fn splitmix64(x: u64) -> u64 {
    let x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn signed_unit(x: u64) -> f32 {
    const SCALE: f32 = 16_777_216.0;
    #[allow(clippy::cast_precision_loss)]
    let zero_to_one = (x >> 40) as f32 / SCALE;
    (zero_to_one * 2.0 - 1.0).clamp(-1.0, 1.0)
}

#[derive(Debug, Default)]
pub(crate) struct FadeTrace {
    fading: std::collections::BTreeMap<u32, (f32, bool)>,
}

impl FadeTrace {
    pub(crate) fn observe(&mut self, frame: u64, opacities: impl Iterator<Item = (u32, f32)>) {
        for (entity, opacity) in opacities {
            match self.fading.get(&entity).copied() {
                None if opacity < 1.0 => {
                    crate::diag::diag!(
                        "fade: entity {entity} starts at frame {frame}, opacity {opacity:.3}"
                    );
                    self.fading.insert(entity, (opacity, true));
                }
                None => {}
                Some(_) if opacity >= 1.0 => {
                    crate::diag::diag!(
                        "fade: entity {entity} ends at frame {frame}, opacity {opacity:.3}"
                    );
                    self.fading.remove(&entity);
                }
                Some((last, true)) if opacity > last => {
                    crate::diag::diag!(
                        "fade: entity {entity} peaks before frame {frame}, opacity {last:.3}"
                    );
                    self.fading.insert(entity, (opacity, false));
                }
                Some((_, falling)) => {
                    self.fading.insert(entity, (opacity, falling));
                }
            }
        }
    }
}

fn construction_conditions(construction: Option<&crate::construction::Construction>) -> Conditions {
    if construction.is_some() {
        Conditions::PARTIALLY_CONSTRUCTED.union(Conditions::ACTIVELY_BEING_CONSTRUCTED)
    } else {
        Conditions::NONE
    }
}

fn site_worker_conditions(worker: Option<&crate::construction::SiteWorker>) -> Conditions {
    use crate::construction::WorkerStage;
    let Some(worker) = worker else {
        return Conditions::NONE;
    };
    let stage = match worker.stage {
        WorkerStage::Constructing => Conditions::ACTIVELY_CONSTRUCTING,
        WorkerStage::Released => Conditions::RUNNING_OFF_MAP,
        WorkerStage::Approaching | WorkerStage::Facing => Conditions::NONE,
    };
    if worker.deployed {
        stage.union(Conditions::DEPLOYED)
    } else {
        stage
    }
}

fn arc_direction(flight: &mint_sage_combat::ProjectileFlight) -> Vec3 {
    let step = flight.step();
    let t = flight.t();
    let (from, to) = if t + step <= 1.0 {
        (t, t + step)
    } else {
        ((t - step).max(0.0), t)
    };
    let a = flight.at(from);
    let b = flight.at(to);
    let delta = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
    let length = (delta.x * delta.x + delta.y * delta.y + delta.z * delta.z).sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return Vec3::new(1.0, 0.0, 0.0);
    }
    Vec3::new(delta.x / length, delta.y / length, delta.z / length)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProjectileView {
    pub(crate) entity: mint_sim::Entity,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) z: f32,
    pub(crate) source: mint_sage_combat::WeaponId,
    pub(crate) nugget: u16,
    pub(crate) direction: Vec3,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct PoseTrack {
    units: Vec<(mint_sim::Entity, Vec2, f32, f32)>,
    projectiles: Vec<(mint_sim::Entity, f32, f32, f32)>,
}

impl PoseTrack {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            units: Vec::new(),
            projectiles: Vec::new(),
        }
    }

    pub(crate) fn capture(&mut self, world: &World, ids: Ids) {
        self.units.clear();
        if let Some(bodies) = world.store(ids.body) {
            self.units.reserve(bodies.len());
            for (entity, body) in bodies.iter() {
                self.units.push((
                    entity,
                    body.0.position,
                    elevation(world, ids, entity),
                    body.0.facing.radians(),
                ));
            }
        }
        self.projectiles.clear();
        if let Some(flights) = world.store(ids.flight) {
            self.projectiles.reserve(flights.len());
            for (entity, flight) in flights.iter() {
                let point = flight.flight.position();
                self.projectiles.push((entity, point.x, point.y, point.z));
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.units.clear();
        self.projectiles.clear();
    }

    #[must_use]
    pub(crate) fn unit(&self, entity: mint_sim::Entity) -> Option<(Vec2, f32, f32)> {
        let found = self
            .units
            .binary_search_by_key(&entity.index(), |(e, ..)| e.index())
            .ok()?;
        let (found_entity, position, z, facing) = *self.units.get(found)?;
        (found_entity == entity).then_some((position, z, facing))
    }

    #[must_use]
    pub(crate) fn projectile(&self, entity: mint_sim::Entity) -> Option<(f32, f32, f32)> {
        let found = self
            .projectiles
            .binary_search_by_key(&entity.index(), |(e, _, _, _)| e.index())
            .ok()?;
        let (found_entity, x, y, z) = *self.projectiles.get(found)?;
        (found_entity == entity).then_some((x, y, z))
    }
}

fn elevation(world: &World, ids: Ids, entity: mint_sim::Entity) -> f32 {
    world.get(ids.elevation, entity).map_or(0.0, |e| e.0)
}

pub(crate) fn step_remembering<R: mint_sim::Rules>(
    simulation: &mut mint_sim::Simulation<R>,
    previous: &mut PoseTrack,
    ids: Ids,
) -> mint_sim::Result<mint_sim::FrameReport> {
    previous.capture(simulation.state(), ids);
    simulation.tick()
}

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub(crate) fn clamp_blend(blend: f32) -> f32 {
    if blend.is_finite() {
        blend.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(crate) fn blend_pose(
    was: (Vec2, f32, f32),
    now: (Vec2, f32, f32),
    blend: f32,
) -> (Vec2, f32, f32) {
    let ((was, was_z, was_facing), (now, now_z, now_facing)) = (was, now);
    let position = Vec2::new(lerp(was.x, now.x, blend), lerp(was.y, now.y, blend));
    let z = lerp(was_z, now_z, blend);
    let delta = Facing::from_radians(was_facing).delta_to(Facing::from_radians(now_facing));
    let facing = Facing::from_radians(was_facing + delta * blend).radians();
    (position, z, facing)
}

pub(crate) struct RenderState<'a> {
    world: &'a World,
    previous: Option<&'a PoseTrack>,
    blend: f32,
}

impl<'a> RenderState<'a> {
    #[must_use]
    pub(crate) const fn new(world: &'a World, blend: f32) -> Self {
        Self {
            world,
            previous: None,
            blend,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn interpolated(world: &'a World, previous: &'a PoseTrack, blend: f32) -> Self {
        Self {
            world,
            previous: Some(previous),
            blend: clamp_blend(blend),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn blend(&self) -> f32 {
        self.blend
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn interpolates(&self) -> bool {
        self.previous.is_some()
    }

    fn blended_pose(
        &self,
        entity: mint_sim::Entity,
        current: (Vec2, f32, f32),
    ) -> (Vec2, f32, f32) {
        match self.previous.and_then(|track| track.unit(entity)) {
            Some(was) => blend_pose(was, current, self.blend),
            None => current,
        }
    }

    #[must_use]
    pub(crate) fn match_clock(&self, id: mint_sim::ComponentId<MatchClock>) -> Option<u64> {
        self.world
            .store(id)
            .and_then(|store| store.iter().next().map(|(_, clock)| clock.0))
    }

    #[must_use]
    fn is_enclosed(&self, ids: Ids, entity: mint_sim::Entity) -> bool {
        self.world.get(ids.contained, entity).is_some_and(|held| {
            held.status
                .has(crate::rules::contain::ContainStatus::ENCLOSED)
        })
    }

    #[must_use]
    pub(crate) fn enclosed(&self, ids: Ids) -> usize {
        self.world.store(ids.contained).map_or(0, |held| {
            held.iter()
                .filter(|(entity, _)| self.is_enclosed(ids, *entity))
                .count()
        })
    }

    #[must_use]
    pub(crate) fn units(
        &self,
        ids: Ids,
        viewer: PlayerId,
        selector: PlayerId,
        weapon_cycle: impl Fn(
            mint_sage_combat::WeaponId,
            mint_sage_combat::WeaponState,
            bool,
            Frame,
            mint_sage_combat::BonusConditions,
        ) -> f32,
        firing_duration: impl Fn(mint_sage_combat::WeaponId) -> LogicFrames,
    ) -> Vec<UnitView> {
        let Some(bodies) = self.world.store(ids.body) else {
            return Vec::new();
        };
        let mut views = Vec::with_capacity(bodies.len());
        let now = Frame::new(self.match_clock(ids.clock).unwrap_or(0));
        let mut flying: Vec<u32> = self
            .world
            .store(ids.flight)
            .map(|flights| {
                flights
                    .iter()
                    .filter(|(_, flight)| flight.object)
                    .filter_map(|(_, flight)| flight.carried.map(|object| object.index()))
                    .collect()
            })
            .unwrap_or_default();
        flying.sort_unstable();
        for (entity, body) in bodies.iter() {
            if flying.binary_search(&entity.index()).is_ok() {
                continue;
            }
            if self.world.contains(ids.concealed, entity) {
                continue;
            }
            if self.is_enclosed(ids, entity) {
                continue;
            }
            if crate::castle::pad_is_occupied(self.world, ids.pad_occupant, ids.doomed, entity) {
                continue;
            }
            let Some(shape) = self.world.get(ids.shape, entity) else {
                continue;
            };
            let owner = self
                .world
                .get(ids.owner, entity)
                .map_or(PlayerId::new(0), |owner| owner.0);
            let armed = crate::weapon_use::armed_view(self.world, ids, entity, &firing_duration);
            let (position, z, facing) = self.blended_pose(
                entity,
                (
                    body.0.position,
                    elevation(self.world, ids, entity),
                    body.0.facing.radians(),
                ),
            );
            views.push(UnitView {
                entity,
                position,
                ground: z + self
                    .world
                    .get(ids.ground_offset, entity)
                    .map_or(0.0, |offset| offset.0),
                facing,
                radius: shape.radius,
                height: shape.height,
                kind_of: shape.kind_of,
                targetable: crate::rules::map_scripts::entity_has_script_status(
                    self.world,
                    ids,
                    entity,
                    crate::rules::map_scripts::ScriptStatusBit::Targetable,
                ),
                pick_type: shape.pick_type(),
                owner,
                selected: self
                    .world
                    .get(ids.selected, entity)
                    .is_some_and(|mask| mask.holds(selector)),
                moving: is_moving(self.world.contains(ids.path, entity), body.0.speed)
                    && !self
                        .world
                        .get(ids.contact, entity)
                        .is_some_and(|contact| contact.blocked()),
                health: self
                    .world
                    .get(ids.health, entity)
                    .filter(|_| !shape.kind_of.contains(KindOf::UNATTACKABLE))
                    .map(|health| health.0.fraction()),
                dying: self.world.contains(ids.doomed, entity),
                created: self
                    .world
                    .get(ids.created, entity)
                    .map_or(0, |created| created.0),
                visibility: self
                    .world
                    .get(ids.seen, entity)
                    .map_or(mint_sage_shroud::ObjectVisibility::Clear, |seen| {
                        seen.visibility(viewer)
                    }),
                speed: body.0.speed,
                weapon_cycle: self.world.get(ids.armed, entity).map_or(0.0, |armed| {
                    let moved = armed.last_shot_at != Some(body.0.position);
                    let conditions =
                        crate::rules::RotwkRules::bonus_conditions(self.world, ids, entity);
                    weapon_cycle(armed.weapon, armed.state, moved, now, conditions)
                }),
                attack_cycle: armed.map_or(0, |weapon| weapon.attack_cycle),
                attack_slot: armed.map(|weapon| weapon.slot),
                kind: self.world.get(ids.kind, entity).map(|kind| kind.0),
                art_kind: self.world.get(ids.kind, entity).map(|kind| kind.0),
                told_kind: self.world.get(ids.kind, entity).map(|kind| kind.0),
                hover_kind: self.world.get(ids.kind, entity).map(|kind| kind.0),
                build_total: self
                    .world
                    .get(ids.construction, entity)
                    .and_then(|site| site.rebuild),
                under_repair: self
                    .world
                    .get(ids.auto_repair, entity)
                    .is_some_and(|repair| {
                        let crate::construction::Repairing::Worker(worker) = repair.repair else {
                            return false;
                        };
                        self.world.get(ids.site_worker, worker).is_some_and(|site| {
                            site.stage == crate::construction::WorkerStage::Constructing
                        })
                    }),
                build_elapsed: self
                    .world
                    .get(ids.construction, entity)
                    .map(|site| site.elapsed),
                held: self.world.contains(ids.held, entity),
                conditions: construction_conditions(self.world.get(ids.construction, entity))
                    .union(if self.world.contains(ids.burning, entity) {
                        Conditions::BURNINGDEATH
                    } else {
                        Conditions::NONE
                    })
                    .union(unit_conditions(
                        is_moving(self.world.contains(ids.path, entity), body.0.speed)
                            && !self
                                .world
                                .get(ids.contact, entity)
                                .is_some_and(|contact| contact.blocked()),
                        self.world
                            .get(ids.selected, entity)
                            .is_some_and(|mask| mask.holds(selector)),
                        self.world.contains(ids.doomed, entity),
                        shape.kind_of.contains(KindOf::STRUCTURE),
                        self.world.contains(ids.engaged, entity),
                        crate::weapon_use::attack_of(
                            self.world,
                            ids,
                            entity,
                            now,
                            &firing_duration,
                        ),
                        armed,
                        now,
                        self.world
                            .get(ids.motive, entity)
                            .map_or(mint_movement::MotionPhase::NONE, |motive| motive.0),
                        self.world
                            .get(ids.doomed, entity)
                            .map_or(Conditions::NONE, |doomed| doomed.death_conditions),
                    ))
                    .union(site_worker_conditions(
                        self.world.get(ids.site_worker, entity),
                    ))
                    .union(self.world.get(ids.forced_model_condition, entity).map_or(
                        Conditions::NONE,
                        |forced| {
                            forced
                                .name
                                .split_whitespace()
                                .filter_map(Conditions::from_token)
                                .fold(Conditions::NONE, |all, one| all.union(one))
                        },
                    )),
            });
        }
        views
    }

    #[must_use]
    pub(crate) fn projectiles(&self, ids: Ids) -> Vec<ProjectileView> {
        let Some(flights) = self.world.store(ids.flight) else {
            return Vec::new();
        };
        flights
            .iter()
            .filter_map(|(entity, flight)| {
                let source = flight.source?;
                let point = flight.flight.position();
                let (x, y, z) = match self.previous.and_then(|track| track.projectile(entity)) {
                    Some((was_x, was_y, was_z)) => (
                        lerp(was_x, point.x, self.blend),
                        lerp(was_y, point.y, self.blend),
                        lerp(was_z, point.z, self.blend),
                    ),
                    None => (point.x, point.y, point.z),
                };
                Some(ProjectileView {
                    entity,
                    x,
                    y,
                    z,
                    source,
                    nugget: flight.nugget,
                    direction: arc_direction(&flight.flight),
                })
            })
            .collect()
    }

    #[must_use]
    pub(crate) fn living_by_player(&self, ids: Ids) -> Vec<(PlayerId, usize)> {
        let Some(health) = self.world.store(ids.health) else {
            return Vec::new();
        };
        let mut counts: Vec<(PlayerId, usize)> = Vec::new();
        for (entity, pool) in health.iter() {
            if pool.0.is_dead() {
                continue;
            }
            let owner = self
                .world
                .get(ids.owner, entity)
                .map_or(PlayerId::new(0), |owner| owner.0);
            match counts.iter_mut().find(|(player, _)| *player == owner) {
                Some((_, count)) => *count = count.saturating_add(1),
                None => counts.push((owner, 1)),
            }
        }
        counts.sort_by_key(|(player, _)| player.get());
        counts
    }

    #[must_use]
    pub(crate) fn selected_count(&self, ids: Ids, local: PlayerId) -> usize {
        self.world.store(ids.selected).map_or(0, |store| {
            store.iter().filter(|(_, mask)| mask.holds(local)).count()
        })
    }
}

#[cfg(test)]
mod tests;
