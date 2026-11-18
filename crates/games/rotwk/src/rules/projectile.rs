use super::*;
use crate::combat::{PendingProjectile, ProjectileKind};
use crate::diag::diag;
use crate::unit::Kind;
use mint_sage_combat::{BezierArc, Point3};
use mint_sage_modules::effects::{names, Effect};

const THROWN: &str = "THROWN_PROJECTILE";

pub(super) const FREEFALL: &str = "FREEFALL";

const SPLATTED: &str = "SPLATTED";

const LANDING_KILL_DAMAGE: &str = "UNRESISTABLE";

const MAX_FALL_TICKS: u16 = 1800;

const GROUND_FRICTION: f32 = 0.15;

const RESTING_SPEED: f32 = 0.01;

const MAX_GROUND_BOUNCES: u8 = 64;

impl RotwkRules {
    pub(super) fn setup_projectiles(&mut self) {
        let bits = Arc::clone(&self.upgrade_bits);
        self.projectile_kinds = (0..self.kinds.len())
            .map(|index| {
                let entry = u16::try_from(index)
                    .ok()
                    .and_then(|index| self.kinds.get(Kind(index)))?;
                let module = entry
                    .record
                    .declared_modules
                    .iter()
                    .find(|module| module.name == "BezierProjectileBehavior")?;
                let read = mint_sage_modules::bezier_projectile_behavior_from_fields(
                    &module.source(&bits),
                    Frame::ZERO,
                )
                .ok()?;
                let arc = |fields: mint_sage_modules::ArcFields| BezierArc {
                    first_height: fields.first_height,
                    second_height: fields.second_height,
                    first_percent_indent: fields.first_percent_indent,
                    second_percent_indent: fields.second_percent_indent,
                    ignore_terrain_height: read.ignore_terrain_height,
                    first_percent_height: read
                        .percent_heights
                        .0
                        .unwrap_or(mint_sage_combat::DEFAULT_FIRST_PERCENT_HEIGHT),
                    second_percent_height: read
                        .percent_heights
                        .1
                        .unwrap_or(mint_sage_combat::DEFAULT_SECOND_PERCENT_HEIGHT),
                    curve_flatten_min_dist: read.curve_flatten_min_dist,
                };
                let weapon = |name: Option<&String>| {
                    name.and_then(|name| self.combat.weapons.id(name.as_str()))
                };
                let landing = |kind: Option<&String>, radius: Option<f32>, time: Option<f32>| {
                    let kind = mint_sage_ini::EmotionType::parse(kind?)?;
                    let radius = radius.filter(|radius| radius.is_finite() && *radius > 0.0)?;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let ms = time
                        .filter(|ms| ms.is_finite() && *ms > 0.0)
                        .map_or(0, |ms| ms.min(3_600_000.0) as u32);
                    Some(crate::combat::LandingEmotion {
                        kind,
                        radius,
                        time: LogicFrames::from_milliseconds(ms),
                    })
                };
                Some(ProjectileKind {
                    pre_landing: landing(
                        read.pre_landing_emotion.as_ref(),
                        read.pre_landing_emotion_radius,
                        read.pre_landing_state_time,
                    ),
                    post_landing: landing(
                        read.post_landing_emotion.as_ref(),
                        read.post_landing_emotion_radius,
                        read.post_landing_state_time,
                    ),
                    name: entry.record.name.clone(),
                    arc: arc(read.arc),
                    bounce_arc: arc(read.bounce_arc),
                    bounce_count: u8::try_from(read.bounce_count).unwrap_or(u8::MAX),
                    bounce_distance: read.bounce_distance,
                    ground_hit_fx: read.ground_hit_fx.clone(),
                    ground_hit_weapon: weapon(read.ground_hit_weapon.as_ref()),
                    ground_bounce_fx: read.ground_bounce_fx.clone(),
                    ground_bounce_weapon: weapon(read.ground_bounce_weapon.as_ref()),
                    die_on_impact: read.die_on_impact,
                    detonate_calls_kill: read.detonate_calls_kill,
                    death_fx: entry
                        .record
                        .declared_modules
                        .iter()
                        .filter(|module| module.name.eq_ignore_ascii_case("FXListDie"))
                        .filter_map(|module| {
                            mint_sage_modules::fx_list_die_from_fields(
                                &module.source(&bits),
                                Frame::ZERO,
                            )
                            .ok()
                        })
                        .find_map(|die| die.death_fx().map(str::to_owned)),
                    flight_path_adjust: read.flight_path_adjust_dist_per_second
                        / Self::ticks_per_second(),
                })
            })
            .collect();
        let declared = self
            .projectile_kinds
            .iter()
            .filter(|kind| kind.is_some())
            .count();
        let throwing = self
            .combat
            .weapons
            .iter()
            .filter(|(_, weapon)| !weapon.projectile_filter_in_container.is_empty())
            .count();
        let flinging = (0..self.kinds.len())
            .filter_map(|index| u16::try_from(index).ok())
            .filter_map(|index| self.kinds.get(Kind(index)))
            .filter(|entry| {
                entry
                    .record
                    .declared_modules
                    .iter()
                    .any(|module| module.name == "FlingPassengerSpecialAbilityUpdate")
            })
            .count();
        println!(
            "projectiles: {declared} template(s) declare a BezierProjectileBehavior; \
             {throwing} weapon(s) throw a passenger (ProjectileFilterInContainer); \
             {flinging} template(s) let theirs go (FlingPassengerSpecialAbilityUpdate)"
        );
        println!("{}", self.describe_debris());
    }

    fn describe_debris(&self) -> String {
        let hz = self.logic_hz();
        let (mut lists, mut by_flag, mut unrun, mut throwable) =
            (0usize, [0usize; 3], 0usize, 0usize);
        for def in self.ocls.iter() {
            let objects = def.create_object.iter().map(|entry| (entry, true));
            let debris = def.create_debris.iter().map(|entry| (entry, false));
            let mut throws = false;
            for (entry, run) in objects.chain(debris) {
                let Some(throw) = crate::summons::Throw::of(entry, hz) else {
                    continue;
                };
                throws = true;
                let flag = match throw {
                    crate::summons::Throw::Flying { .. } => 0,
                    crate::summons::Throw::Up { .. } => 1,
                    crate::summons::Throw::RandomForce { .. } => 2,
                };
                if let Some(count) = by_flag.get_mut(flag) {
                    *count = count.saturating_add(1);
                }
                if !run {
                    unrun = unrun.saturating_add(1);
                } else if entry.names.iter().any(|name| {
                    self.kinds
                        .kind(name)
                        .is_some_and(|kind| self.physics_of(kind).is_some())
                }) {
                    throwable = throwable.saturating_add(1);
                }
            }
            lists = lists.saturating_add(usize::from(throws));
        }
        let [flying, up, random] = by_flag;
        format!(
            "debris: {lists} creation list(s) throw what they make - {flying} SEND_IT_FLYING, \
             {up} SEND_IT_UP, {random} RANDOM_FORCE nugget(s); {unrun} of them CreateDebris, \
             which mint does not run; {throwable} name a template in this match with a \
             PhysicsBehavior"
        )
    }

    pub(super) fn projectile_kind(&self, kind: Kind) -> Option<&ProjectileKind> {
        self.projectile_kinds.get(usize::from(kind.0))?.as_ref()
    }

    pub(super) fn launch_flight(
        &self,
        pending: &PendingProjectile,
        launcher: Entity,
        carried: Option<(Entity, Kind)>,
    ) -> Flight {
        let kind = match carried {
            Some((_, kind)) => Some(kind),
            None => self
                .projectile_template(pending.source, pending.nugget)
                .and_then(|name| self.kinds.kind(name)),
        };
        let resolved = kind.and_then(|kind| self.projectile_kind(kind));
        let arc = resolved.map_or_else(BezierArc::default, |kind| kind.arc);
        let (from, to) = (pending.from, pending.to);
        let terrain = self.highest_ground_between(from, to);
        let flight = ProjectileFlight::launch(
            from,
            to,
            arc,
            terrain,
            pending.speed,
            Self::ticks_per_second(),
        );
        if let Some(kind) = resolved {
            let points = flight.control_points();
            diag!(
                "projectile: {} arc h={},{} indent={}%,{}% frames={} ends z={:.1},{:.1}                  terrain={:.1} control z={:.1},{:.1}",
                kind.name,
                arc.first_height,
                arc.second_height,
                arc.first_percent_indent * 100.0,
                arc.second_percent_indent * 100.0,
                flight.frames(),
                from.z,
                to.z,
                terrain.unwrap_or(f32::NAN),
                points[1].z,
                points[2].z
            );
        }
        Flight {
            kind,
            speed: pending.speed,
            bounces_done: 0,
            flight,
            warhead: Some(pending.warhead),
            launcher,
            victim: pending.victim,
            struck: pending.struck,
            source: Some(pending.source),
            nugget: pending.nugget,
            passenger_roll: None,
            carried: carried.map(|(entity, _)| entity),
            carried_velocity: Point3::default(),
            carried_ballistic: false,
            falls_to_death: false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn drop_passenger(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        holder: Entity,
        passenger: Entity,
        velocity: [f32; 3],
        warhead: Option<WeaponId>,
        falls_to_death: bool,
    ) -> Result<()> {
        let world = tick.state();
        let (Some(body), Some(kind)) = (
            world.get(ids.body, passenger).copied(),
            world.get(ids.kind, passenger).copied(),
        ) else {
            return Ok(());
        };
        let ahead = world
            .get(ids.body, holder)
            .map_or(body.0.facing, |held| held.0.facing)
            .to_vector();
        let from = Point3::new(
            body.0.position.x,
            body.0.position.y,
            world
                .get(ids.elevation, passenger)
                .map_or(0.0, |elevation| elevation.0),
        );
        let [forward, left, up] = velocity;
        let hz = Self::ticks_per_second();
        let per_tick = Point3::new(
            (ahead.x * forward - ahead.y * left) / hz,
            (ahead.y * forward + ahead.x * left) / hz,
            up / hz,
        );
        let flight = Flight {
            falls_to_death,
            ..self.carried_flight(from, per_tick, (passenger, kind), holder, warhead)
        };
        crate::perf::note_projectile_spawned();
        let entity = tick.state_mut().spawn()?;
        self.launch_passenger(tick, ids, holder, passenger, &flight)?;
        tick.state_mut().insert(ids.flight, entity, flight)?;
        self.apply_weather_to_new_object(tick, ids, entity)?;
        Ok(())
    }

    fn carried_flight(
        &self,
        from: Point3,
        velocity: Point3,
        (carried, kind): (Entity, Kind),
        launcher: Entity,
        warhead: Option<WeaponId>,
    ) -> Flight {
        let (end, frames) = self.fall(from, velocity);
        let seconds = f32::from(frames) / Self::ticks_per_second();
        Flight {
            kind: Some(kind),
            speed: end.distance_squared_3d(from).sqrt() / seconds,
            bounces_done: 0,
            flight: ProjectileFlight::ballistic(from, velocity, end, frames),
            warhead,
            launcher,
            victim: None,
            struck: false,
            source: None,
            nugget: u16::MAX,
            passenger_roll: None,
            carried: Some(carried),
            carried_velocity: velocity,
            carried_ballistic: true,
            falls_to_death: false,
        }
    }

    fn fall(&self, from: Point3, velocity: Point3) -> (Point3, u16) {
        let on_ground = |x: f32, y: f32| Point3::new(x, y, self.ground_height(x, y));
        if self.gravity >= 0.0 || !from.is_finite() || !velocity.is_finite() {
            return (on_ground(from.x, from.y), 1);
        }
        let mut last = on_ground(from.x, from.y);
        for tick in 1..=MAX_FALL_TICKS {
            let t = f32::from(tick);
            let (x, y) = (from.x + velocity.x * t, from.y + velocity.y * t);
            last = on_ground(x, y);
            if from.z + velocity.z * t + 0.5 * self.gravity * t * t <= last.z {
                return (last, tick);
            }
        }
        (last, MAX_FALL_TICKS)
    }

    pub(super) fn launch_passenger(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        passenger: Entity,
        flight: &Flight,
    ) -> Result<()> {
        self.take_out(tick, ids, container, passenger)?;
        self.carry(tick, ids, passenger, flight.flight.position())?;
        self.apply_effects(
            tick,
            ids,
            vec![(
                passenger,
                Effect::ModelConditions {
                    add: names(vec![airborne(flight).to_owned()]),
                    remove: names(Vec::new()),
                },
            )],
        )?;
        let (from, to) = (flight.flight.start(), flight.flight.end());
        diag!(
            "projectile: frame {} entity {} launches its passenger entity {} from ({:.1},{:.1},{:.1}) to ({:.1},{:.1},{:.1}), {} frames",
            tick.frame().get(),
            container.index(),
            passenger.index(),
            from.x,
            from.y,
            from.z,
            to.x,
            to.y,
            to.z,
            flight.flight.frames()
        );
        Ok(())
    }

    pub(super) fn leave_flight(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let riding: Vec<(Entity, Flight)> = tick
            .state()
            .store(ids.flight)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, flight)| flight.carried == Some(entity))
                    .map(|(carrier, flight)| (carrier, *flight))
                    .collect()
            })
            .unwrap_or_default();
        for (carrier, flight) in riding {
            tick.state_mut().despawn(carrier);
            tick.state_mut().remove(ids.ground_offset, entity);
            self.apply_effects(
                tick,
                ids,
                vec![(
                    entity,
                    Effect::ModelConditions {
                        add: names(Vec::new()),
                        remove: names(vec![airborne(&flight).to_owned()]),
                    },
                )],
            )?;
        }
        Ok(())
    }

    fn carry(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity, here: Point3) -> Result<()> {
        if let Some(mut body) = tick.state().get(ids.body, entity).copied() {
            body.0.position = Vec2::new(here.x, here.y);
            body.0.speed = 0.0;
            body.0.waypoint = 0;
            tick.state_mut().insert(ids.body, entity, body)?;
        }
        tick.state_mut().remove(ids.path, entity);
        let ground = self.ground_height(here.x, here.y);
        tick.state_mut()
            .insert(ids.elevation, entity, crate::unit::Elevation::new(ground))?;
        tick.state_mut().insert(
            ids.ground_offset,
            entity,
            crate::unit::GroundOffset(here.z - ground),
        )?;
        Ok(())
    }

    fn end_flight(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        flight: &Flight,
        dies: bool,
    ) -> Result<()> {
        tick.state_mut().despawn(entity);
        let Some(carried) = flight.carried else {
            return Ok(());
        };
        tick.state_mut().remove(ids.ground_offset, carried);
        self.apply_effects(
            tick,
            ids,
            vec![(
                carried,
                Effect::ModelConditions {
                    add: names(Vec::new()),
                    remove: names(vec![airborne(flight).to_owned()]),
                },
            )],
        )?;
        let alive = tick
            .state()
            .get(ids.health, carried)
            .is_some_and(|health| !health.0.is_dead());
        let here = flight.flight.position();
        let rests_dead = self.above_ground(here).z <= 0.0
            && tick
                .state()
                .get(ids.kind, carried)
                .and_then(|kind| self.physics_of(*kind))
                .is_some_and(|physics| physics.kill_when_resting_on_ground());
        let splatted = flight.falls_to_death && self.above_ground(here).z <= 0.0;
        let fate = match (alive, dies, rests_dead) {
            (false, _, _) => "is already dead",
            _ if splatted => "dies of the fall (SPLATTED)",
            (true, true, _) => "dies (DieOnImpact)",
            (true, false, true) => "dies (KillWhenRestingOnGround, SPLATTED)",
            (true, false, false) => "stands",
        };
        if flight.warhead.is_some() {
            diag!(
                "projectile: frame {} thrown entity {} comes down at ({:.1},{:.1}) and {fate}",
                tick.frame().get(),
                carried.index(),
                here.x,
                here.y
            );
        } else {
            diag!(
                "debris: frame {} entity {} {} lands at ({:.1},{:.1},{:.1}) after {} ticks and {fate}",
                tick.frame().get(),
                carried.index(),
                self.kind_name(tick.state().get(ids.kind, carried).copied()),
                here.x,
                here.y,
                here.z,
                flight.flight.frames()
            );
        }
        if alive && (splatted || dies || rests_dead) {
            self.land_dead(tick, ids, carried, splatted || rests_dead)?;
        }
        Ok(())
    }

    fn land_dead(&self, tick: &mut Tick<'_>, ids: Ids, body: Entity, splatted: bool) -> Result<()> {
        let Some(mut health) = tick.state().get(ids.health, body).copied() else {
            return Ok(());
        };
        let dealt = health.0.current();
        if !health.0.kill() {
            return Ok(());
        }
        tick.state_mut().insert(ids.health, body, health)?;
        if splatted {
            tick.state_mut().insert(
                ids.killing_blow,
                body,
                super::death::KillingBlow {
                    killer: None,
                    damage_type: self.combat.damage_types.lookup(LANDING_KILL_DAMAGE),
                    dealt,
                    death: crate::combat::DeathNaming::Normal,
                    sub_type: None,
                },
            )?;
            self.apply_effects(
                tick,
                ids,
                vec![(
                    body,
                    Effect::ModelConditions {
                        add: names(vec![SPLATTED.to_owned()]),
                        remove: names(Vec::new()),
                    },
                )],
            )?;
        }
        self.resolve_deaths(tick, ids, &[(body, 0.0)])
    }

    pub(super) fn physics_of(&self, kind: Kind) -> Option<mint_sage_modules::PhysicsBehavior> {
        let module = self
            .kinds
            .get(kind)?
            .record
            .declared_modules
            .iter()
            .rev()
            .find(|module| module.name.eq_ignore_ascii_case("PhysicsBehavior"))?;
        mint_sage_modules::physics_behavior_from_fields(
            &module.source(&self.upgrade_bits),
            Frame::ZERO,
        )
        .ok()
    }

    fn ground_bounce(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        flight: &Flight,
        here: Point3,
    ) -> Option<Flight> {
        if !flight.carried_ballistic
            || self.gravity >= 0.0
            || flight.bounces_done >= MAX_GROUND_BOUNCES
        {
            return None;
        }
        let carried = flight.carried?;
        let physics = tick
            .state()
            .get(ids.kind, carried)
            .copied()
            .and_then(|kind| self.physics_of(kind))?;
        if !physics.allow_bouncing() {
            return None;
        }
        let drop = here.z - flight.flight.start().z;
        let impact_vz = (flight.carried_velocity.z * flight.carried_velocity.z
            + 2.0 * self.gravity * drop)
            .max(0.0)
            .sqrt();
        let bounced = Point3::new(
            flight.carried_velocity.x * (1.0 - GROUND_FRICTION),
            flight.carried_velocity.y * (1.0 - GROUND_FRICTION),
            self.ground_stiffness * impact_vz,
        );
        if bounced.x.abs() < RESTING_SPEED
            && bounced.y.abs() < RESTING_SPEED
            && bounced.z.abs() < RESTING_SPEED
        {
            return None;
        }
        let (end, frames) = self.fall(here, bounced);
        let mut next = *flight;
        next.flight = ProjectileFlight::ballistic(here, bounced, end, frames);
        next.carried_velocity = bounced;
        next.bounces_done = flight.bounces_done.saturating_add(1);
        Some(next)
    }

    pub(super) fn launch_debris(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        (entity, kind): (Entity, Kind),
        from: Point3,
        throw: [f32; 3],
    ) -> Result<()> {
        let hz = Self::ticks_per_second();
        let logic_hz = self.logic_hz();
        let lift = self.gravity * hz * hz / (2.0 * logic_hz);
        let velocity = Point3::new(
            throw[0] * logic_hz / hz,
            throw[1] * logic_hz / hz,
            (throw[2] * logic_hz + lift) / hz,
        );
        self.carry(tick, ids, entity, from)?;
        let flight = self.carried_flight(from, velocity, (entity, kind), entity, None);
        let end = flight.flight.end();
        diag!(
            "debris: frame {} entity {} {} leaves ({:.1},{:.1},{:.1}) at ({:.2},{:.2},{:.2})/logic frame, \
             ({:.1},{:.1},{:.1})/s, {} ticks to ({:.1},{:.1},{:.1})",
            tick.frame().get(),
            entity.index(),
            self.kind_name(Some(kind)),
            from.x,
            from.y,
            from.z,
            throw[0],
            throw[1],
            throw[2],
            velocity.x * hz,
            velocity.y * hz,
            velocity.z * hz,
            flight.flight.frames(),
            end.x,
            end.y,
            end.z
        );
        crate::perf::note_projectile_spawned();
        let carrier = tick.state_mut().spawn()?;
        tick.state_mut().insert(ids.flight, carrier, flight)?;
        self.apply_weather_to_new_object(tick, ids, carrier)?;
        Ok(())
    }

    pub(super) fn logic_hz(&self) -> f32 {
        self.combat
            .authored_logic_hz
            .and_then(|hz| u16::try_from(hz.get()).ok())
            .map_or_else(Self::ticks_per_second, f32::from)
    }

    fn highest_ground_between(&self, from: Point3, to: Point3) -> Option<f32> {
        mint_sage_combat::highest_terrain_along(from, to, |x, y| {
            self.passability().heights.height_at(x, y)
        })
    }

    fn above_ground(&self, point: Point3) -> Point3 {
        Point3::new(
            point.x,
            point.y,
            point.z - self.ground_height(point.x, point.y),
        )
    }

    pub(super) fn advance_projectiles(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        let flights: Vec<(Entity, Flight)> = tick
            .state()
            .store(ids.flight)
            .map(|store| store.iter().map(|(e, f)| (e, *f)).collect())
            .unwrap_or_default();
        if flights.is_empty() {
            return Ok(());
        }
        let living = self.living(tick, ids);
        for (entity, mut flight) in flights {
            if let Some(carried) = flight.carried {
                let alive = tick
                    .state()
                    .get(ids.health, carried)
                    .is_some_and(|health| !health.0.is_dead());
                if !alive {
                    self.end_flight(tick, ids, entity, &flight, false)?;
                    continue;
                }
            }
            if flight.bounces_done == 0 {
                self.track_victim(tick, ids, &mut flight);
            }
            let before = flight.flight.ticks_to_arrival();
            let arrived = flight.flight.advance();
            self.pre_landing_emotion(tick, ids, entity, &flight, before)?;
            let here = flight.flight.position();
            if let Some(carried) = flight.carried {
                self.carry(tick, ids, carried, here)?;
            }
            let collided = if flight.bounces_done == 0 && !flight.falls_to_death {
                self.collided(tick, ids, &flight, here, &living)
            } else {
                None
            };
            let kind = flight
                .kind
                .filter(|_| !flight.falls_to_death)
                .and_then(|kind| self.projectile_kind(kind))
                .cloned();
            if let Some(foe) = collided {
                diag!(
                    "projectile: collided at ({:.1},{:.1}) {:.1} above ground with {} {:?}{}",
                    here.x,
                    here.y,
                    self.above_ground(here).z,
                    tick.state()
                        .get(ids.kind, foe.entity)
                        .and_then(|kind| self.kinds.get(*kind))
                        .map_or("?", |entry| entry.record.name.as_str()),
                    foe.entity,
                    if flight.victim == Some(foe.entity) {
                        " (its victim)"
                    } else {
                        ""
                    }
                );
                self.detonate(tick, ids, &flight, here, &living, events)?;
                if let Some(kind) = &kind {
                    self.projectile_death_fx(tick, ids, &flight, here, kind);
                }
                let dies = kind.as_ref().is_some_and(|kind| kind.die_on_impact);
                self.end_flight(tick, ids, entity, &flight, dies)?;
                continue;
            }
            if !arrived {
                tick.state_mut().insert(ids.flight, entity, flight)?;
                continue;
            }
            if flight.bounces_done == 0 {
                self.detonate(tick, ids, &flight, here, &living, events)?;
            }
            let Some(kind) = kind else {
                if let Some(bounced) = self.ground_bounce(tick, ids, &flight, here) {
                    let carried_index = flight
                        .carried
                        .map_or_else(|| entity.index(), |carried| carried.index());
                    let carried_kind = flight
                        .carried
                        .and_then(|carried| tick.state().get(ids.kind, carried).copied());
                    diag!(
                        "debris: frame {} entity {} {} bounces at ({:.1},{:.1},{:.1}) leaving at \
                         ({:.2},{:.2},{:.2})/tick, bounce {}",
                        tick.frame().get(),
                        carried_index,
                        self.kind_name(carried_kind),
                        here.x,
                        here.y,
                        here.z,
                        bounced.carried_velocity.x,
                        bounced.carried_velocity.y,
                        bounced.carried_velocity.z,
                        bounced.bounces_done
                    );
                    tick.state_mut().insert(ids.flight, entity, bounced)?;
                } else {
                    self.end_flight(tick, ids, entity, &flight, false)?;
                }
                continue;
            };
            let remaining = kind.bounce_count.saturating_sub(flight.bounces_done);
            let (fx, weapon) = if remaining > 0 {
                (kind.ground_bounce_fx.as_deref(), kind.ground_bounce_weapon)
            } else {
                (kind.ground_hit_fx.as_deref(), kind.ground_hit_weapon)
            };
            diag!(
                "projectile: ground hit at ({:.1},{:.1}) fx={} bounces={}",
                here.x,
                here.y,
                fx.unwrap_or("none"),
                kind.bounce_count
            );
            if let Some(list) = fx {
                self.emit_fx_named(
                    tick,
                    ids,
                    list,
                    here,
                    crate::fx::FxSource::Weapon,
                    flight.launcher,
                );
            }
            if let Some(weapon) = weapon {
                let shot = Shot {
                    weapon,
                    attacker: flight.launcher,
                    origin: here,
                    victim: None,
                    missed: None,
                    centre: Vec2::new(here.x, here.y),
                    centre_height: here.z,
                    owner_if_gone: None,
                };
                let result = self.fire_shot(tick, ids, &shot, &living)?;
                self.emit_weapon_fx(
                    tick,
                    ids,
                    weapon,
                    here,
                    flight.launcher,
                    flight.launcher,
                    None,
                    None,
                )?;
                self.apply_fire_result(tick, ids, weapon, &shot, result, events)?;
            }
            if remaining == 0 {
                self.projectile_death_fx(tick, ids, &flight, here, &kind);
                self.end_flight(tick, ids, entity, &flight, kind.die_on_impact)?;
                continue;
            }
            let Some(next) = bounce_from(here, flight.flight.start(), kind.bounce_distance) else {
                self.end_flight(tick, ids, entity, &flight, kind.die_on_impact)?;
                continue;
            };
            flight.bounces_done = flight.bounces_done.saturating_add(1);
            flight.flight = ProjectileFlight::launch(
                here,
                next,
                kind.bounce_arc,
                self.highest_ground_between(here, next),
                flight.speed * 0.5,
                Self::ticks_per_second(),
            );
            tick.state_mut().insert(ids.flight, entity, flight)?;
        }
        Ok(())
    }

    fn pre_landing_emotion(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        flight: &Flight,
        before: u32,
    ) -> Result<()> {
        let Some(landing) = flight
            .kind
            .and_then(|kind| self.projectile_kind(kind))
            .and_then(|kind| kind.pre_landing)
        else {
            return Ok(());
        };
        let lead = landing.time.get();
        if !(before > lead && flight.flight.ticks_to_arrival() <= lead) {
            return Ok(());
        }
        let end = flight.flight.end();
        let filter = crate::defeat::ObjectFilter::parse(&["ANY".to_owned()]);
        self.emotion_pulse(
            tick,
            ids,
            entity,
            super::emotions::Pulse {
                at: mint_sage_modules::PowerPlace::At(end.x, end.y),
                radius: landing.radius,
                kind: landing.kind,
                filter: &filter,
                lasts: super::emotions::ONE_FRAME,
            },
        )
    }

    fn post_landing_emotion(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        flight: &Flight,
        here: Point3,
    ) -> Result<()> {
        let Some(landing) = flight
            .kind
            .and_then(|kind| self.projectile_kind(kind))
            .and_then(|kind| kind.post_landing)
        else {
            return Ok(());
        };
        let filter = crate::defeat::ObjectFilter::parse(&["ANY".to_owned()]);
        self.emotion_pulse(
            tick,
            ids,
            flight.launcher,
            super::emotions::Pulse {
                at: mint_sage_modules::PowerPlace::At(here.x, here.y),
                radius: landing.radius,
                kind: landing.kind,
                filter: &filter,
                lasts: landing.time,
            },
        )
    }

    fn collided(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        flight: &Flight,
        here: Point3,
        living: &[(Foe, PlayerId)],
    ) -> Option<Foe> {
        let mask = self
            .combat
            .weapons
            .get(flight.warhead?)?
            .projectile_collides;
        let state = tick.state();
        let owner = state.get(ids.owner, flight.launcher).map(|owner| owner.0);
        let holder = state
            .get(ids.contained, flight.launcher)
            .map(|held| held.container);
        let at = Vec2::new(here.x, here.y);
        let ground = self.ground_height(here.x, here.y);
        let stops = |foe: &Foe, player: PlayerId| {
            if flight.struck && flight.victim == Some(foe.entity) {
                return true;
            }
            if state
                .get(ids.shape, foe.entity)
                .is_some_and(|shape| shape.kind_of.contains(KindOf::NO_COLLIDE))
            {
                return false;
            }
            let kind_of: &[String] = state
                .get(ids.kind, foe.entity)
                .and_then(|kind| self.kinds.get(*kind))
                .map_or(&[], |entry| &entry.record.kind_of);
            let has = |flag: &str| kind_of.iter().any(|written| written == flag);
            let relation = if owner.is_some() {
                self.object_victim_relation(state, flight.launcher, foe.entity)
            } else {
                mint_sage_combat::Relation::Neutral
            };
            mask.collides(
                relation,
                mint_sage_combat::CollideClass {
                    structure: foe.is_structure,
                    controlled: owner == Some(player),
                    shrubbery: has("SHRUBBERY"),
                    projectile: has("PROJECTILE"),
                    small_missile: has("SMALL_MISSILE"),
                    ballistic_missile: has("BALLISTIC_MISSILE"),
                    monster: has("MONSTER"),
                    wall: false,
                },
            )
        };
        living
            .iter()
            .filter(|(foe, _)| {
                foe.entity != flight.launcher
                    && Some(foe.entity) != holder
                    && Some(foe.entity) != flight.carried
                    && !foe.unattackable
            })
            .filter(|(foe, _)| {
                here.z >= foe.z.min(ground)
                    && here.z <= foe.z + foe.height
                    && at.minus(foe.position).length_squared() <= foe.radius * foe.radius
            })
            .filter(|(foe, player)| stops(foe, *player))
            .map(|(foe, _)| *foe)
            .min_by(|a, b| {
                at.minus(a.position)
                    .length_squared()
                    .total_cmp(&at.minus(b.position).length_squared())
            })
    }

    fn track_victim(&self, tick: &Tick<'_>, ids: Ids, flight: &mut Flight) {
        let Some(kind) = flight.kind.and_then(|kind| self.projectile_kind(kind)) else {
            return;
        };
        if kind.flight_path_adjust <= 0.0 || !flight.struck {
            return;
        }
        let Some(victim) = flight
            .victim
            .and_then(|victim| self.foe_of(tick, ids, victim))
        else {
            return;
        };
        flight.flight.track(
            victim.aim_point(),
            kind.flight_path_adjust,
            kind.arc,
            |from, to| self.highest_ground_between(from, to),
        );
    }

    fn projectile_death_fx(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        flight: &Flight,
        here: Point3,
        kind: &ProjectileKind,
    ) {
        if !kind.detonate_calls_kill {
            return;
        }
        let Some(list) = kind.death_fx.as_deref() else {
            return;
        };
        diag!(
            "projectile: death FX {list} at ({:.1},{:.1})",
            here.x,
            here.y
        );
        self.emit_fx_named(
            tick,
            ids,
            list,
            here,
            crate::fx::FxSource::Weapon,
            flight.launcher,
        );
    }

    fn detonate(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        flight: &Flight,
        here: Point3,
        living: &[(Foe, PlayerId)],
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        self.post_landing_emotion(tick, ids, flight, here)?;
        let Some(warhead) = flight.warhead else {
            return Ok(());
        };
        let hit_stored_target = self
            .combat
            .weapons
            .get(warhead)
            .is_some_and(|warhead| warhead.hit_stored_target);
        let victim = if let Some(carried) = flight.carried {
            self.foe_of(tick, ids, carried)
        } else if hit_stored_target {
            living
                .iter()
                .find(|(foe, _)| flight.struck && flight.victim == Some(foe.entity))
                .map(|(foe, _)| *foe)
        } else {
            None
        };
        let above = self.above_ground(here);
        let shot = Shot {
            weapon: warhead,
            attacker: flight.launcher,
            origin: here,
            victim: victim.map(|foe| (foe.entity, foe.position, foe.aim_point().z)),
            missed: None,
            centre: Vec2::new(here.x, here.y),
            centre_height: here.z,
            owner_if_gone: None,
        };
        diag!(
            "projectile: detonated at ({:.1},{:.1}) {:.1} above ground victim={}",
            here.x,
            here.y,
            above.z,
            if victim.is_some() { "struck" } else { "none" }
        );
        let mut result = self.fire_shot(tick, ids, &shot, living)?;
        let start = flight.flight.start();
        let launched = flight
            .source
            .and_then(|source| self.launched_by(source, Vec2::new(start.x, start.y)));
        if let Some(launched) = launched {
            for hit in &mut result.damage {
                hit.launched = Some(launched);
            }
            let mut struck: Vec<Entity> = result.damage.iter().map(|hit| hit.victim).collect();
            struck.sort_unstable_by_key(|entity| entity.index());
            struck.dedup();
            self.landing_debris(tick, ids, launched, Vec2::new(here.x, here.y), &struck)?;
        }
        if let (Some(struck), Some(container)) = (flight.passenger_roll, flight.victim) {
            let (on_container, rest): (Vec<DamageEvent>, Vec<DamageEvent>) =
                core::mem::take(&mut result.damage)
                    .into_iter()
                    .partition(|hit| hit.victim == container);
            result.damage = rest;
            if struck {
                for hit in &on_container {
                    events.extend(self.passenger_hit(tick.state(), ids, hit));
                }
            }
            events.extend(on_container);
        }
        self.emit_weapon_fx(
            tick,
            ids,
            warhead,
            here,
            flight.launcher,
            flight.launcher,
            victim.map(|foe| foe.position),
            victim.map(|foe| foe.entity),
        )?;
        self.apply_fire_result(tick, ids, warhead, &shot, result, events)
    }
}

fn bounce_from(here: Point3, came_from: Point3, distance: f32) -> Option<Point3> {
    if !distance.is_finite() || distance <= 0.0 {
        return None;
    }
    let (dx, dy) = (here.x - came_from.x, here.y - came_from.y);
    let length = (dx * dx + dy * dy).sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return None;
    }
    Some(Point3::new(
        here.x + dx / length * distance,
        here.y + dy / length * distance,
        here.z,
    ))
}

fn airborne(flight: &Flight) -> &'static str {
    if flight.falls_to_death {
        FREEFALL
    } else {
        THROWN
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_bounce_carries_on_along_the_heading_it_arrived_on() {
        let came_from = Point3::new(0.0, 0.0, 0.0);
        let here = Point3::new(100.0, 0.0, 12.0);
        let next = bounce_from(here, came_from, 40.0).unwrap();
        assert_eq!((next.x, next.y), (140.0, 0.0));
        assert!(bounce_from(here, here, 40.0).is_none());
        assert!(bounce_from(here, came_from, 0.0).is_none());
    }
}
