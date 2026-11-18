use crate::spell_state::PendingImpact;
use mint_sage_combat::{Point3, ProjectileFlight};
use mint_sage_modules::{Effect, ObjectRecord};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Waiting,
    Airborne,
    Stunned,
    StandingUp,
}

impl Phase {
    const fn tag(self) -> u8 {
        match self {
            Phase::Waiting => 0,
            Phase::Airborne => 1,
            Phase::Stunned => 2,
            Phase::StandingUp => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Knocked {
    launch: Point3,
    pub(crate) flight: Option<ProjectileFlight>,
    pub(crate) ground: f32,
    pub(crate) had_ground: bool,
    pub(crate) until: Frame,
    pub(crate) phase: Phase,
    pub(crate) age: u16,
}

impl Component for Knocked {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.launch.x);
        hasher.write_f32(self.launch.y);
        hasher.write_f32(self.launch.z);
        hasher.write_bool(self.flight.is_some());
        if let Some(flight) = &self.flight {
            flight.hash_into(hasher);
            hasher.write_u16(self.age);
        }
        hasher.write_f32(self.ground);
        hasher.write_bool(self.had_ground);
        self.until.hash_into(hasher);
        hasher.write_u8(self.phase.tag());
    }
}

impl Knocked {
    pub(crate) const fn off_its_feet(&self) -> bool {
        !matches!(self.phase, Phase::Waiting)
    }
}

const FLAILING: &str = "STUNNED_FLAILING";

const STUNNED: &str = "STUNNED";
const STANDING_UP: &str = "STUNNED_STANDING_UP";

const FREEFALL: &str = "FREEFALL";
const SPLATTED: &str = "SPLATTED";
const DYING: &str = "DYING";

const STUN_LOW_MS: f32 = 1000.0;
const STUN_HIGH_MS: f32 = 2000.0;
const STANDING_MS: f32 = 1000.0;

const MAX_FALL_FRAMES: u32 = 3 * crate::game_data::RETAIL_LOGIC_HZ;

const MIN_FLING_POINTS: u32 = 3;

const FLING_STEP: f32 = 10.0;

const ROLL_RANGE: u32 = 10_000;

pub(crate) fn taper(distance: f32, radius: f32, taper_off: f32) -> f32 {
    if radius <= 0.0 || !radius.is_finite() || !distance.is_finite() {
        return 0.0;
    }
    let share = (distance / radius).clamp(0.0, 1.0);
    let mult = 1.0 - share * (1.0 - taper_off);
    mult.clamp(0.0, 1.0)
}

pub(crate) fn inside_arc(facing: Vec2, direction: Vec2, arc: Option<f32>, inverted: bool) -> bool {
    let Some(arc) = arc.filter(|arc| arc.is_finite()) else {
        return true;
    };
    let (facing, direction) = (unit(facing), unit(direction));
    let (Some(facing), Some(direction)) = (facing, direction) else {
        return true;
    };
    let dot = facing.x * direction.x + facing.y * direction.y;
    let within = dot >= mint_math::f32::cos(arc.clamp(0.0, 180.0).to_radians());
    within != inverted
}

pub(crate) fn bend_shock(offset: Vec2, factor: f32) -> Vec2 {
    if factor == 0.0 || !factor.is_finite() {
        return offset;
    }
    let c = factor.min(1.0);
    let swirl = c * c;
    Vec2::new(
        offset.x * (1.0 - c) + offset.y * swirl,
        offset.y * (1.0 - c) - offset.x * swirl,
    )
}

fn unit(v: Vec2) -> Option<Vec2> {
    let length = v.length();
    if !length.is_finite() || length <= 1e-4 {
        return None;
    }
    Some(Vec2::new(v.x / length, v.y / length))
}

impl RotwkRules {
    pub(super) fn begin_knockback(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        weapon: WeaponId,
        shot: &Shot,
        hits: &[(Entity, usize)],
    ) -> Result<()> {
        if hits.is_empty() {
            return Ok(());
        }
        let facing = tick
            .state()
            .get(ids.body, shot.attacker)
            .map_or(Vec2::new(1.0, 0.0), |body| body.0.facing.to_vector());
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        let mut held: Vec<PendingImpact> = Vec::new();
        let mut thrown = 0usize;
        let mut radius = 0.0f32;
        for &(victim, index) in hits {
            let Some(mint_sage_combat::Nugget::MetaImpact(impact)) = self
                .combat
                .weapons
                .get(weapon)
                .and_then(|template| template.nuggets.get(index))
            else {
                continue;
            };
            radius = impact.radius;
            if !self.meta_impact_accepts(tick, ids, victim, impact) {
                continue;
            }
            if impact.only_when_just_died {
                if let Ok(nugget) = u16::try_from(index) {
                    held.push(PendingImpact {
                        attacker: shot.attacker,
                        victim,
                        weapon,
                        nugget,
                        centre: shot.centre,
                    });
                }
                continue;
            }
            if self.throw_one(
                tick,
                ids,
                shot.centre,
                facing,
                shot.attacker,
                victim,
                impact,
                &mut effects,
            )? {
                thrown = thrown.saturating_add(1);
            }
        }
        if thrown > 0 {
            diag!(
                "knockback: {thrown} object(s) thrown from ({:.0},{:.0}) over r={radius:.0}",
                shot.centre.x,
                shot.centre.y
            );
        }
        self.hold_impacts(tick, ids, held)?;
        self.apply_effects(tick, ids, effects)
    }

    fn hold_impacts(&self, tick: &mut Tick<'_>, ids: Ids, held: Vec<PendingImpact>) -> Result<()> {
        if held.is_empty() {
            return Ok(());
        }
        let Some(clock) = tick
            .state()
            .store(ids.clock)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let mut queue = tick
            .state()
            .get(ids.pending_impacts, clock)
            .cloned()
            .unwrap_or_default();
        queue.0.extend(held);
        tick.state_mut().insert(ids.pending_impacts, clock, queue)?;
        Ok(())
    }

    pub(super) fn resolve_pending_impacts(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        killed: &[(Entity, f32)],
    ) -> Result<()> {
        let Some(clock) = tick
            .state()
            .store(ids.clock)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(queue) = tick.state_mut().remove(ids.pending_impacts, clock) else {
            return Ok(());
        };
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for held in queue.0 {
            if !killed.iter().any(|(victim, _)| *victim == held.victim) {
                continue;
            }
            let Some(mint_sage_combat::Nugget::MetaImpact(impact)) = self
                .combat
                .weapons
                .get(held.weapon)
                .and_then(|template| template.nuggets.get(usize::from(held.nugget)))
            else {
                continue;
            };
            let facing = tick
                .state()
                .get(ids.body, held.attacker)
                .map_or(Vec2::new(1.0, 0.0), |body| body.0.facing.to_vector());
            self.throw_one(
                tick,
                ids,
                held.centre,
                facing,
                held.attacker,
                held.victim,
                impact,
                &mut effects,
            )?;
        }
        self.apply_effects(tick, ids, effects)
    }

    fn meta_impact_accepts(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        victim: Entity,
        impact: &mint_sage_combat::MetaImpactNugget,
    ) -> bool {
        let Some(record) = self.record_of(tick, ids, victim) else {
            return false;
        };
        let has = |kind: &str| record.kind_of.iter().any(|held| held == kind);
        if has("TREE") || has("STRUCTURE") {
            return false;
        }
        if has("HORDE") && !impact.affect_hordes {
            return false;
        }
        let cyclonic_ship = impact.cyclonic_factor != 0.0 && has("SHIP");
        if !cyclonic_ship
            && (self.resists_shockwave(tick, ids, victim, 100.0)
                || self.resists_shockwave(tick, ids, victim, impact.amount))
        {
            return false;
        }
        if let Some(held) = tick.state().get(ids.contained, victim) {
            if !tick.state().contains(ids.horde, held.container) || impact.affect_hordes {
                return false;
            }
        }
        true
    }

    pub(super) fn shove(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        shot: &Shot,
        victim: Entity,
        impact: &mint_sage_combat::MetaImpactNugget,
    ) -> Result<bool> {
        let facing = tick
            .state()
            .get(ids.body, shot.attacker)
            .map_or(Vec2::new(1.0, 0.0), |body| body.0.facing.to_vector());
        let mut effects = Vec::new();
        let thrown = self.throw_one(
            tick,
            ids,
            shot.centre,
            facing,
            shot.attacker,
            victim,
            impact,
            &mut effects,
        )?;
        self.apply_effects(tick, ids, effects)?;
        Ok(thrown)
    }

    #[allow(clippy::too_many_arguments)]
    fn throw_one(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        centre: Vec2,
        facing: Vec2,
        attacker: Entity,
        victim: Entity,
        impact: &mint_sage_combat::MetaImpactNugget,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<bool> {
        let now = tick.frame();
        let Some(body) = tick.state().get(ids.body, victim).copied() else {
            return Ok(false);
        };
        let offset = body.0.position.minus(centre);
        if !inside_arc(facing, offset, impact.arc, impact.arc_inverted) {
            return Ok(false);
        }
        if !impact.kill_filter.is_empty()
            && self.record_of(tick, ids, victim).is_some_and(|record| {
                ObjectFilter::parse(&impact.kill_filter).passes(&record.kind_of, &record.name)
            })
        {
            self.kill_by_meta_impact(tick, ids, attacker, victim)?;
            return Ok(false);
        }
        if !self.knockback_admits(tick, ids, victim, impact) {
            return Ok(false);
        }
        if self.resists_shockwave(tick, ids, victim, impact.amount) {
            return Ok(false);
        }
        self.sound_shock_impact(tick, ids, victim, impact.cyclonic_factor.abs() > 0.0);
        if self.stands_against_knockback(tick, ids, victim, impact.hero_resist) {
            return Ok(false);
        }
        let signed = if impact.invert && !impact.clear_radius {
            Vec2::new(-offset.x, -offset.y)
        } else {
            offset
        };
        let away = unit(bend_shock(signed, impact.cyclonic_factor)).unwrap_or(Vec2::new(0.0, 0.0));
        let distance = offset.length();
        let force = impact.amount * taper(distance, impact.radius, impact.taper_off);
        diag!(
            "knockback: r={:.0} taper={:.2} victim at d={distance:.1} force={force:.1}/s",
            impact.radius,
            impact.taper_off
        );
        let wave = self.wave_arrival(now, distance, impact.speed);
        let r = Self::shock_scale(tick);
        self.launch(
            tick,
            ids,
            victim,
            away,
            force * r,
            impact.z_mult * r,
            wave,
            effects,
        )
    }

    fn kill_by_meta_impact(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) -> Result<()> {
        let Some(mut health) = tick.state().get(ids.health, victim).copied() else {
            return Ok(());
        };
        let dealt = health.0.current();
        if !health.0.kill() {
            return Ok(());
        }
        tick.state_mut().insert(ids.health, victim, health)?;
        tick.state_mut().insert(
            ids.killing_blow,
            victim,
            super::death::KillingBlow {
                killer: Some(attacker),
                damage_type: None,
                dealt,
                death: crate::combat::DeathNaming::Normal,
                sub_type: None,
            },
        )?;
        diag!(
            "knockback: entity {} killed outright by KillObjectFilter",
            victim.index()
        );
        Ok(())
    }

    pub(super) fn crush_throw(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        away: Vec2,
        victim: Entity,
        amount: f32,
        z_factor: f32,
    ) -> Result<bool> {
        if !tick.state().contains(ids.body, victim) {
            return Ok(false);
        }
        if self.stays_put(tick, ids, victim)
            || tick.state().contains(ids.knocked, victim)
            || self.resists_shockwave(tick, ids, victim, 100.0)
        {
            return Ok(false);
        }
        let mut effects = Vec::new();
        let now = tick.frame();
        let thrown = self.launch(tick, ids, victim, away, amount, z_factor, now, &mut effects)?;
        self.apply_effects(tick, ids, effects)?;
        Ok(thrown)
    }

    pub(super) fn ram_throw(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        from: Vec2,
        victim: Entity,
        power: f32,
        z_mult: f32,
    ) -> Result<bool> {
        let Some(position) = tick
            .state()
            .get(ids.body, victim)
            .map(|body| body.0.position)
        else {
            return Ok(false);
        };
        if !power.is_finite()
            || power <= 0.0
            || self.stays_put(tick, ids, victim)
            || tick.state().contains(ids.knocked, victim)
            || self.resists_shockwave(tick, ids, victim, 100.0)
        {
            return Ok(false);
        }
        self.sound_shock_impact(tick, ids, victim, false);
        if self.bonuses_of(tick.state(), ids, victim).resist_knockback >= 1.0 {
            return Ok(false);
        }
        let Some(away) = unit(position.minus(from)) else {
            return Ok(false);
        };
        let r = Self::shock_scale(tick);
        let mut effects = Vec::new();
        let now = tick.frame();
        let thrown = self.launch(
            tick,
            ids,
            victim,
            away,
            power * r,
            z_mult * r,
            now,
            &mut effects,
        )?;
        self.apply_effects(tick, ids, effects)?;
        Ok(thrown)
    }

    fn sound_shock_impact(&self, tick: &mut Tick<'_>, ids: Ids, victim: Entity, cyclonic: bool) {
        if tick.state().contains(ids.knocked, victim)
            || self.physics_of_entity(tick, ids, victim).is_none()
        {
            return;
        }
        let kind = if cyclonic {
            crate::voice::VoiceKind::ImpactCyclonic
        } else {
            crate::voice::VoiceKind::Impact
        };
        self.emit_voice_with(tick, ids, victim, kind, None, false);
    }

    fn shock_scale(tick: &mut Tick<'_>) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let share = tick.rng().below(ROLL_RANGE) as f32 / ROLL_RANGE as f32;
        0.85 + 0.3 * share
    }

    #[allow(clippy::too_many_arguments)]
    fn launch(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
        away: Vec2,
        force: f32,
        z_mult: f32,
        until: Frame,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<bool> {
        #[allow(clippy::cast_precision_loss)]
        let speed = force / crate::game_data::RETAIL_LOGIC_HZ as f32;
        self.launch_per_frame(tick, ids, victim, away, speed, z_mult, until, effects)
    }

    #[allow(clippy::too_many_arguments)]
    fn launch_per_frame(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
        away: Vec2,
        speed: f32,
        z_mult: f32,
        until: Frame,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<bool> {
        if !speed.is_finite() || speed <= 0.0 || self.physics_of_entity(tick, ids, victim).is_none()
        {
            return Ok(false);
        }
        if tick
            .state()
            .get(ids.contained, victim)
            .is_some_and(|held| !tick.state().contains(ids.horde, held.container))
        {
            return Ok(false);
        }
        let now = tick.frame();
        let ground = tick.state().get(ids.ground_offset, victim).map(|g| g.0);
        let mut knocked = Knocked {
            launch: Point3::new(away.x * speed, away.y * speed, speed * z_mult),
            flight: None,
            ground: ground.unwrap_or(0.0),
            had_ground: ground.is_some(),
            until,
            age: 0,
            phase: if until > now {
                Phase::Waiting
            } else {
                Phase::Airborne
            },
        };
        if knocked.phase == Phase::Airborne {
            knocked.flight = self.plan_fling(tick, ids, victim, &knocked);
            effects.push((victim, conditions(&[FLAILING], &[])));
        }
        tick.state_mut().insert(ids.knocked, victim, knocked)?;
        Ok(true)
    }

    pub(super) fn shock_admits(&self, tick: &Tick<'_>, ids: Ids, victim: Entity) -> bool {
        let world = tick.state();
        self.physics_of_entity(tick, ids, victim).is_some()
            && !world.contains(ids.knocked, victim)
            && !(self.resists_shockwave(tick, ids, victim, 100.0)
                || world
                    .get(ids.contained, victim)
                    .is_some_and(|held| !world.contains(ids.horde, held.container)))
    }

    pub(super) fn script_shock(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
        angle: f32,
        force: f32,
        lift: f32,
    ) -> Result<bool> {
        if !self.shock_admits(tick, ids, victim) {
            return Ok(false);
        }
        let (sin, cos) = mint_math::f32::sin_cos(angle * (std::f32::consts::PI / 180.0));
        let mut effects = Vec::new();
        let now = tick.frame();
        let thrown = self.launch_per_frame(
            tick,
            ids,
            victim,
            Vec2::new(cos, sin),
            force,
            lift,
            now,
            &mut effects,
        )?;
        if thrown {
            self.apply_effects(tick, ids, effects)?;
        }
        Ok(thrown)
    }

    fn physics_of_entity(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        victim: Entity,
    ) -> Option<mint_sage_modules::PhysicsBehavior> {
        self.physics_of(*tick.state().get(ids.kind, victim)?)
    }

    fn plan_fling(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        victim: Entity,
        knocked: &Knocked,
    ) -> Option<ProjectileFlight> {
        let physics = self.physics_of_entity(tick, ids, victim)?;
        let at = tick.state().get(ids.body, victim)?.0.position;
        let ticks = (SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ).max(1);
        #[allow(clippy::cast_precision_loss)]
        let per = ticks as f32;
        let gravity = self.gravity * per * per;
        let g = physics.gravity_mult().unwrap_or(1.0) * gravity;
        if !g.is_finite() || g >= 0.0 {
            return None;
        }
        let offset = tick
            .state()
            .get(ids.ground_offset, victim)
            .map_or(knocked.ground, |g| g.0);
        let h0 = (offset - knocked.ground).max(0.0);
        let mut fall = 0u32;
        let mut height = h0;
        while fall < MAX_FALL_FRAMES && height > 0.0 {
            #[allow(clippy::cast_precision_loss)]
            let step = g * fall as f32;
            height += step;
            fall = fall.saturating_add(1);
        }
        let v = knocked.launch;
        let magnitude = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
        let mut rise = v.z;
        if 0.25 * magnitude > rise && fall < 2 {
            rise = 0.25 * magnitude;
        }
        let up = (2.0 * rise / g).abs();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let up = if up.is_finite() {
            (up.min(f32::from(u16::MAX)) as u32).max(1)
        } else {
            1
        };
        let frames = up.saturating_add(fall);
        #[allow(clippy::cast_precision_loss)]
        let span = frames as f32;
        let apex = (rise * span / 4.0).max(0.0);
        let horizontal = Vec2::new(v.x * span, v.y * span);
        let landing = self.adjust_fling_destination(tick, ids, victim, at, at.plus(horizontal));
        let start = Point3::new(at.x, at.y, self.ground_height(at.x, at.y) + offset);
        let end = Point3::new(
            landing.x,
            landing.y,
            self.ground_height(landing.x, landing.y) + knocked.ground,
        );
        let speed = fling_speed(v, rise, span, apex, h0).max(-gravity);
        let points = self.fling_points(&physics.fling_arc(), start, end, apex);
        let length = ProjectileFlight::through(points, 1).curve_length();
        let count = (length / speed).ceil() + 1.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = if count.is_finite() {
            (count.min(f32::from(u16::MAX)) as u32).max(MIN_FLING_POINTS)
        } else {
            MIN_FLING_POINTS
        };
        let total =
            u16::try_from(count.saturating_sub(1).saturating_mul(ticks)).unwrap_or(u16::MAX);
        diag!(
            "knockback: {victim:?} flung {frames} frame(s), apex {apex:.1}, {count} point(s), lands ({:.0},{:.0})",
            landing.x,
            landing.y
        );
        Some(ProjectileFlight::through(points, total))
    }

    fn fling_points(
        &self,
        arc: &mint_sage_combat::BezierArc,
        start: Point3,
        end: Point3,
        apex: f32,
    ) -> [Point3; 4] {
        let apex = apex.max(0.5);
        let drop = (start.z - end.z).max(0.0);
        let share = ((apex - drop) / apex).max(0.0);
        let first_rise = arc.first_height * apex;
        let second_rise = arc.second_height * apex * share;
        let first = start.lerp(end, arc.first_percent_indent);
        let second = start.lerp(end, arc.second_percent_indent);
        let (first_z, second_z) = if arc.ignore_terrain_height {
            let rise = end.z - start.z;
            (
                start.z + rise * arc.first_percent_height,
                start.z + rise * arc.second_percent_height,
            )
        } else {
            let terrain = mint_sage_combat::highest_terrain_along(start, end, |x, y| {
                self.passability.heights.height_at(x, y)
            })
            .unwrap_or(f32::MIN);
            if arc.curve_flatten_min_dist > 0.0 {
                let scale =
                    (start.distance_squared_3d(end).sqrt() / arc.curve_flatten_min_dist).min(1.0);
                (
                    first.z.max(terrain) + first_rise * scale,
                    second.z.max(terrain) + second_rise * scale,
                )
            } else {
                let base = terrain.max(start.z).max(end.z);
                (base + first_rise, base + second_rise)
            }
        };
        [
            start,
            Point3::new(first.x, first.y, first_z),
            Point3::new(second.x, second.y, second_z),
            end,
        ]
    }

    fn adjust_fling_destination(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        victim: Entity,
        from: Vec2,
        to: Vec2,
    ) -> Vec2 {
        let mover = self.mover_class(tick, ids, victim);
        let view = self.dynamic_view(tick, ids);
        let stands = |point: Vec2| {
            self.passability
                .cell_at(point)
                .is_some_and(|cell| mint_movement::Passability::is_passable(&view, cell, mover))
        };
        let line = to.minus(from);
        let steps = (line.length() / FLING_STEP).ceil();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = if steps.is_finite() {
            (steps.min(1024.0) as u32).max(1)
        } else {
            1
        };
        let mut last = from;
        for step in 1..=steps {
            #[allow(clippy::cast_precision_loss)]
            let share = step as f32 / steps as f32;
            let point = from.plus(Vec2::new(line.x * share, line.y * share));
            if !stands(point) {
                return last;
            }
            last = point;
        }
        to
    }

    fn wave_arrival(&self, now: Frame, distance: f32, speed: f32) -> Frame {
        if !speed.is_finite() || speed <= 0.0 || !distance.is_finite() || distance <= 0.0 {
            return now;
        }
        let frames = (distance / speed * Self::ticks_per_second()).ceil();
        if !frames.is_finite() || frames <= 0.0 {
            return now;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = frames.min(f32::from(u16::MAX)) as u32;
        now.checked_add(LogicFrames::from_frames(frames))
            .unwrap_or(Frame::FOREVER)
    }

    fn knockback_admits(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        victim: Entity,
        impact: &mint_sage_combat::MetaImpactNugget,
    ) -> bool {
        if self.stays_put(tick, ids, victim) {
            return false;
        }
        if impact.special_filter.is_empty() {
            return true;
        }
        let filter = ObjectFilter::parse(&impact.special_filter);
        self.record_of(tick, ids, victim)
            .is_some_and(|record| filter.passes(&record.kind_of, &record.name))
    }

    fn stays_put(&self, tick: &Tick<'_>, ids: Ids, victim: Entity) -> bool {
        tick.state().get(ids.shape, victim).is_none_or(|shape| {
            shape.kind_of.contains(KindOf::STRUCTURE) || shape.kind_of.contains(KindOf::IMMOBILE)
        })
    }

    fn resists_shockwave(&self, tick: &Tick<'_>, ids: Ids, victim: Entity, amount: f32) -> bool {
        let resistance = self
            .record_of(tick, ids, victim)
            .and_then(|record| record.shockwave_resistance)
            .unwrap_or(0.0);
        amount <= resistance
    }

    fn stands_against_knockback(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
        hero_resist: f32,
    ) -> bool {
        let resist = self.bonuses_of(tick.state(), ids, victim).resist_knockback;
        if self.roll_resists(tick, resist) {
            return true;
        }
        if !hero_resist.is_finite() || hero_resist <= 0.0 {
            return false;
        }
        let hero = self
            .record_of(tick, ids, victim)
            .is_some_and(|record| record.kind_of.iter().any(|kind| kind == "HERO"));
        hero && self.roll_resists(tick, hero_resist)
    }

    fn roll_resists(&self, tick: &mut Tick<'_>, resist: f32) -> bool {
        if !resist.is_finite() || resist <= 0.0 {
            return false;
        }
        if resist >= 1.0 {
            return true;
        }
        let roll = tick.rng().below(ROLL_RANGE);
        #[allow(clippy::cast_precision_loss)]
        let share = roll as f32 / ROLL_RANGE as f32;
        share < resist
    }

    fn shock_times(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> (LogicFrames, LogicFrames, LogicFrames) {
        let physics = self.physics_of_entity(tick, ids, entity);
        let frames = |written: Option<f32>, default: f32| {
            let ms = written
                .filter(|ms| ms.is_finite() && *ms >= 0.0)
                .unwrap_or(default);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            mint_sage_modules::retail_duration_ticks(ms.min(f32::from(u16::MAX)) as u32)
        };
        let written = |read: fn(&mint_sage_modules::PhysicsBehavior) -> Option<f32>| {
            physics.as_ref().and_then(read)
        };
        (
            frames(
                written(mint_sage_modules::PhysicsBehavior::shock_stunned_time_low),
                STUN_LOW_MS,
            ),
            frames(
                written(mint_sage_modules::PhysicsBehavior::shock_stunned_time_high),
                STUN_HIGH_MS,
            ),
            frames(
                written(mint_sage_modules::PhysicsBehavior::shock_standing_time),
                STANDING_MS,
            ),
        )
    }

    fn roll_stun(&self, tick: &mut Tick<'_>, low: LogicFrames, high: LogicFrames) -> LogicFrames {
        let (low, high) = if high.get() > low.get() {
            (low, high)
        } else {
            (high, low)
        };
        let span = high.get().saturating_sub(low.get());
        if span == 0 {
            return high;
        }
        LogicFrames::from_frames(
            low.get()
                .saturating_add(tick.rng().below(span.saturating_add(1))),
        )
    }

    pub(super) fn record_of<'a>(
        &'a self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<&'a ObjectRecord> {
        let kind = tick.state().get(ids.kind, entity).copied()?;
        self.kinds.get(kind).map(|entry| &entry.record)
    }

    pub(super) fn advance_knockback(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let thrown: Vec<Entity> = tick
            .state()
            .store(ids.knocked)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        if thrown.is_empty() {
            return Ok(());
        }
        let now = tick.frame();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for entity in thrown {
            let Some(mut knocked) = tick.state().get(ids.knocked, entity).copied() else {
                continue;
            };
            match knocked.phase {
                Phase::Waiting => {
                    if now < knocked.until {
                        continue;
                    }
                    knocked.phase = Phase::Airborne;
                    knocked.flight = self.plan_fling(tick, ids, entity, &knocked);
                    effects.push((entity, conditions(&[FLAILING], &[])));
                    tick.state_mut().insert(ids.knocked, entity, knocked)?;
                }
                Phase::Stunned | Phase::StandingUp => {
                    if now < knocked.until {
                        continue;
                    }
                    if knocked.phase == Phase::Stunned {
                        let standing = self.shock_times(tick, ids, entity).2;
                        if !standing.is_zero() {
                            knocked.phase = Phase::StandingUp;
                            knocked.until = now.checked_add(standing).unwrap_or(Frame::FOREVER);
                            effects.push((entity, conditions(&[STANDING_UP], &[STUNNED])));
                            tick.state_mut().insert(ids.knocked, entity, knocked)?;
                            continue;
                        }
                    }
                    diag!(
                        "knockback: {entity:?} back on its feet on frame {}",
                        now.get()
                    );
                    effects.push((entity, conditions(&[], &[STUNNED, STANDING_UP])));
                    tick.state_mut().remove(ids.knocked, entity);
                }
                Phase::Airborne => {
                    let Some(mut body) = tick.state().get(ids.body, entity).copied() else {
                        tick.state_mut().remove(ids.knocked, entity);
                        continue;
                    };
                    let sit = u16::try_from(SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ)
                        .unwrap_or(1)
                        .max(1);
                    knocked.age = knocked.age.saturating_add(1);
                    let age = knocked.age;
                    let orients = self
                        .physics_of_entity(tick, ids, entity)
                        .is_some_and(|physics| physics.orients_to_flight_path());
                    let landed = match knocked.flight.as_mut() {
                        Some(flight) => {
                            if age > sit {
                                flight.advance();
                            }
                            let landed =
                                age >= flight.frames().saturating_add(sit.saturating_mul(2));
                            let at = flight.position();
                            let moved = Vec2::new(at.x, at.y).minus(body.0.position);
                            if orients {
                                if let Some(heading) = mint_movement::Facing::from_vector(moved) {
                                    body.0.facing = heading;
                                }
                            }
                            body.0.position = Vec2::new(at.x, at.y);
                            let ground = self.ground_height(at.x, at.y);
                            tick.state_mut().insert(
                                ids.ground_offset,
                                entity,
                                crate::unit::GroundOffset((at.z - ground).max(knocked.ground)),
                            )?;
                            landed
                        }
                        None => true,
                    };
                    body.0.speed = 0.0;
                    tick.state_mut().insert(ids.body, entity, body)?;
                    if landed {
                        self.land(tick, ids, entity, knocked, &mut effects)?;
                    } else {
                        tick.state_mut().insert(ids.knocked, entity, knocked)?;
                    }
                }
            }
        }
        self.apply_effects(tick, ids, effects)
    }

    fn land(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        mut knocked: Knocked,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        if knocked.had_ground {
            tick.state_mut().insert(
                ids.ground_offset,
                entity,
                crate::unit::GroundOffset(knocked.ground),
            )?;
        } else {
            tick.state_mut().remove(ids.ground_offset, entity);
        }
        if let Some(list) = self
            .physics_of_entity(tick, ids, entity)
            .and_then(|physics| physics.ground_hit_fx().map(str::to_owned))
        {
            effects.push((
                entity,
                Effect::PlayFx {
                    list: list.as_str().into(),
                    at: mint_sage_modules::PowerPlace::Source,
                    on_object: false,
                    secondary: None,
                },
            ));
        }
        if self.holds_condition(tick, entity, FREEFALL) {
            effects.push((entity, conditions(&[SPLATTED], &[FREEFALL, FLAILING])));
        }
        let now = tick.frame();
        let dead = tick.state().contains(ids.doomed, entity)
            || tick
                .state()
                .get(ids.health, entity)
                .is_some_and(|health| health.0.is_dead());
        if dead {
            diag!(
                "knockback: {entity:?} lands dead on frame {}, SPLATTED",
                now.get()
            );
            effects.push((
                entity,
                conditions(&[DYING, SPLATTED], &[FLAILING, FREEFALL]),
            ));
            tick.state_mut().remove(ids.knocked, entity);
            return Ok(());
        }
        let (low, high, _) = self.shock_times(tick, ids, entity);
        let stunned = self.roll_stun(tick, low, high);
        if stunned.is_zero() {
            effects.push((entity, conditions(&[], &[FLAILING, FREEFALL])));
            tick.state_mut().remove(ids.knocked, entity);
            return Ok(());
        }
        diag!(
            "knockback: {entity:?} landed on frame {}, stunned {} frame(s)",
            now.get(),
            stunned.get()
        );
        knocked.phase = Phase::Stunned;
        knocked.until = now.checked_add(stunned).unwrap_or(Frame::FOREVER);
        effects.push((entity, conditions(&[STUNNED], &[FLAILING, FREEFALL])));
        tick.state_mut().insert(ids.knocked, entity, knocked)?;
        Ok(())
    }
}

pub(crate) fn fling_speed(v: Point3, rise: f32, span: f32, apex: f32, h0: f32) -> f32 {
    let (x, y, z) = (v.x * span, v.y * span, span * rise / 2.0);
    ((x * x + y * y + z * z).sqrt() + 2.0 * apex + h0) / span
}

fn conditions(add: &[&str], remove: &[&str]) -> Effect {
    Effect::ModelConditions {
        add: mint_sage_modules::effects::names(add.iter().map(|s| (*s).to_owned()).collect()),
        remove: mint_sage_modules::effects::names(remove.iter().map(|s| (*s).to_owned()).collect()),
    }
}
