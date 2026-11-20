use std::borrow::Cow;

use mint_movement::Vec2;
use mint_sage_ini::{EmotionAiState, EmotionIndex, EmotionNuggetDef, EmotionType};
use mint_sage_modules::object_filter::Standing;
use mint_sage_modules::{
    frightens, points_at, taunt_excluded, Effect, EmotionTrackerUpdate, PowerPlace,
};
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick, World};

use super::emotion_states::EmotionState;
use super::{Ids, RotwkRules};
use crate::combat::{Foe, TargetGrid};
use crate::diag::diag;

pub(super) const TRACKER_MODULE: &str = "EmotionTrackerUpdate";

#[derive(Debug)]
pub(crate) struct KindEmotions {
    declarations: Vec<mint_sage_ini::Block>,
    nuggets: Vec<EmotionNuggetDef>,
}

impl KindEmotions {
    pub(super) fn build(tracker: &EmotionTrackerUpdate, index: &EmotionIndex) -> Self {
        Self {
            declarations: tracker.emotions().to_vec(),
            nuggets: resolve_nuggets(index, tracker),
        }
    }

    fn declared_by(&self, tracker: &EmotionTrackerUpdate) -> bool {
        self.declarations.as_slice() == tracker.emotions()
    }
}

fn resolve_nuggets(index: &EmotionIndex, tracker: &EmotionTrackerUpdate) -> Vec<EmotionNuggetDef> {
    let errors = mint_sage_ini::ValueErrors::new();
    let nuggets = tracker
        .emotions()
        .iter()
        .filter_map(|declaration| index.resolve(declaration, &errors))
        .collect();
    let _ = errors.take();
    nuggets
}

const TYPES: usize = 12;

pub(super) const TICKS_PER_FRAME: u32 = mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;

const CHEER_PULSE_FRAMES: u32 = 10;

const HERO_CHEER_COOLDOWN: LogicFrames = LogicFrames::from_engine_milliseconds(60_000);

const RESIST_IMMUNE: f32 = 0.5;

pub(crate) const ONE_FRAME: LogicFrames = LogicFrames::from_frames(TICKS_PER_FRAME);

pub(super) struct Pulse<'a> {
    pub(super) at: PowerPlace,
    pub(super) radius: f32,
    pub(super) kind: EmotionType,
    pub(super) filter: &'a crate::defeat::ObjectFilter,
    pub(super) lasts: LogicFrames,
    pub(super) enemies_only: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Pending {
    set: bool,
    expiry: Frame,
    source: Option<Entity>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Forced {
    kind: u8,
    frames: u32,
    source: Option<Entity>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct NuggetClock {
    next_allowed: Frame,
    by_object: Vec<(Entity, Frame)>,
    by_template: Vec<(u32, Frame)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct EmotionTable {
    pending: [Pending; TYPES],
    forced: Option<Forced>,
    hero_cooldown: Frame,
    hero_latch: bool,
    scarer: Option<Entity>,
    point_target: Option<Entity>,
    hero: Option<Entity>,
    hurt: Option<Frame>,
    clocks: Vec<NuggetClock>,
}

impl EmotionTable {
    fn pending(&self, kind: u8) -> Pending {
        self.pending
            .get(usize::from(kind))
            .copied()
            .unwrap_or_default()
    }

    fn pending_mut(&mut self, kind: u8) -> Option<&mut Pending> {
        self.pending.get_mut(usize::from(kind))
    }

    pub(super) fn clear(&mut self, kind: EmotionType) {
        if let Some(entry) = self.pending_mut(kind.code()) {
            entry.set = false;
        }
    }

    pub(super) fn hurt_within(&self, window: LogicFrames, now: Frame) -> bool {
        self.hurt
            .is_some_and(|hurt| hurt.checked_add(window).is_some_and(|end| now < end))
    }

    fn clock(&self, nugget: usize) -> Option<&NuggetClock> {
        self.clocks.get(nugget)
    }

    fn clock_mut(&mut self, nugget: usize) -> Option<&mut NuggetClock> {
        if self.clocks.len() <= nugget {
            self.clocks
                .resize(nugget.saturating_add(1), NuggetClock::default());
        }
        self.clocks.get_mut(nugget)
    }
}

impl Component for EmotionTable {
    fn hash_into(&self, hasher: &mut StateHasher) {
        let entity = |hasher: &mut StateHasher, held: Option<Entity>| {
            hasher.write_bool(held.is_some());
            hasher.write_u32(held.map_or(0, Entity::index));
        };
        for entry in &self.pending {
            hasher.write_bool(entry.set);
            hasher.write_u64(entry.expiry.get());
            entity(hasher, entry.source);
        }
        hasher.write_bool(self.forced.is_some());
        if let Some(forced) = self.forced {
            hasher.write_u8(forced.kind);
            hasher.write_u32(forced.frames);
            entity(hasher, forced.source);
        }
        hasher.write_u64(self.hero_cooldown.get());
        hasher.write_bool(self.hero_latch);
        entity(hasher, self.scarer);
        entity(hasher, self.point_target);
        entity(hasher, self.hero);
        hasher.write_u64(self.hurt.map_or(u64::MAX, Frame::get));
        hasher.write_usize(self.clocks.len());
        for clock in &self.clocks {
            hasher.write_u64(clock.next_allowed.get());
            hasher.write_usize(clock.by_object.len());
            for (cause, until) in &clock.by_object {
                hasher.write_u32(cause.index());
                hasher.write_u64(until.get());
            }
            hasher.write_usize(clock.by_template.len());
            for (kind, until) in &clock.by_template {
                hasher.write_u32(*kind);
                hasher.write_u64(until.get());
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Emotion {
    pub(crate) nugget: u16,
    pub(crate) kind: u8,
    pub(crate) cause: Option<Entity>,
    pub(crate) activated: Frame,
    pub(crate) ends: Frame,
    pub(crate) lock_until: Frame,
    pub(crate) prevents_commands: bool,
    pub(crate) attribute_applied: bool,
    pub(crate) state: Option<EmotionState>,
}

impl Component for Emotion {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u16(self.nugget);
        hasher.write_u8(self.kind);
        hasher.write_bool(self.cause.is_some());
        hasher.write_u32(self.cause.map_or(0, Entity::index));
        hasher.write_u64(self.activated.get());
        hasher.write_u64(self.ends.get());
        hasher.write_u64(self.lock_until.get());
        hasher.write_bool(self.prevents_commands);
        hasher.write_bool(self.attribute_applied);
        hasher.write_bool(self.state.is_some());
        if let Some(state) = &self.state {
            state.hash_into(hasher);
        }
    }
}

fn frame_after(now: Frame, ms: u32) -> Frame {
    let retail_frames = u32::try_from(
        (u64::from(ms) * u64::from(crate::game_data::RETAIL_LOGIC_HZ)).div_ceil(1000),
    )
    .unwrap_or(u32::MAX);
    now.checked_add(LogicFrames::from_frames(
        retail_frames.saturating_mul(TICKS_PER_FRAME),
    ))
    .unwrap_or(Frame::FOREVER)
}

fn retail_frames(duration: LogicFrames) -> u32 {
    let per = TICKS_PER_FRAME;
    duration.get().div_ceil(per.max(1))
}

#[derive(Clone, Copy)]
struct Notable {
    entity: Entity,
    position: Vec2,
    radius: f32,
    player: PlayerId,
    scary: bool,
    hero: bool,
    airborne: bool,
}

#[derive(Clone, Copy)]
struct ThreatSource {
    position: Vec2,
    player: PlayerId,
    value: f32,
    range: f32,
}

pub(super) struct Surroundings<'a> {
    living: &'a [(Foe, PlayerId)],
    notable: Vec<Notable>,
    threats: Vec<ThreatSource>,
}

#[derive(Clone, Copy, Default)]
struct Threats {
    enemy: i64,
    friend: i64,
}

impl RotwkRules {
    pub(super) fn tracker_holder(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        horde_first: bool,
    ) -> Option<Entity> {
        let wiring = self.modules.as_ref()?;
        let has = |candidate: Entity| world.contains(wiring.emotion_state, candidate);
        let horde = world
            .get(ids.member, entity)
            .map(|member| member.horde)
            .filter(|horde| world.contains(ids.horde, *horde) && has(*horde));
        if horde_first {
            horde.or_else(|| has(entity).then_some(entity))
        } else {
            has(entity).then_some(entity).or(horde)
        }
    }

    pub(super) fn has_negative_emotion(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        let Some(holder) = self.tracker_holder(world, ids, entity, true) else {
            return false;
        };
        world
            .get(ids.emotion, holder)
            .and_then(|emotion| EmotionType::from_code(emotion.kind))
            .is_some_and(EmotionType::is_negative)
    }

    pub(crate) fn pulse_emotion(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        target: Entity,
        kind: EmotionType,
        source: Option<Entity>,
        lasts: LogicFrames,
    ) -> Result<()> {
        let Some(holder) = self.tracker_holder(tick.state(), ids, target, true) else {
            return Ok(());
        };
        let expiry = tick
            .frame()
            .checked_add(lasts.max(ONE_FRAME))
            .unwrap_or(Frame::FOREVER);
        self.write_pending(tick, ids, holder, kind, source, expiry)
    }

    fn write_pending(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        holder: Entity,
        kind: EmotionType,
        source: Option<Entity>,
        expiry: Frame,
    ) -> Result<()> {
        let mut table = tick
            .state()
            .get(ids.emotion_table, holder)
            .cloned()
            .unwrap_or_default();
        if let Some(entry) = table.pending_mut(kind.code()) {
            *entry = Pending {
                set: true,
                expiry,
                source,
            };
        }
        tick.state_mut().insert(ids.emotion_table, holder, table)?;
        Ok(())
    }

    pub(crate) fn force_emotion(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        target: Entity,
        kind: EmotionType,
        lasts: LogicFrames,
        source: Option<Entity>,
    ) -> Result<()> {
        let Some(holder) = self.tracker_holder(tick.state(), ids, target, false) else {
            return Ok(());
        };
        let mut table = tick
            .state()
            .get(ids.emotion_table, holder)
            .cloned()
            .unwrap_or_default();
        table.forced = Some(Forced {
            kind: kind.code(),
            frames: retail_frames(lasts),
            source,
        });
        tick.state_mut().insert(ids.emotion_table, holder, table)?;
        let mut effects = Vec::new();
        self.deactivate(tick, ids, holder, &mut effects)?;
        self.apply_effects(tick, ids, effects)
    }

    pub(super) fn note_emotion_hurt(&self, tick: &mut Tick<'_>, ids: Ids, victim: Entity) {
        let Some(holder) = self.tracker_holder(tick.state(), ids, victim, true) else {
            return;
        };
        let now = tick.frame();
        if let Some(table) = tick.state_mut().get_mut(ids.emotion_table, holder) {
            table.hurt = Some(now);
        } else {
            let table = EmotionTable {
                hurt: Some(now),
                ..EmotionTable::default()
            };
            let _ = tick.state_mut().insert(ids.emotion_table, holder, table);
        }
    }

    pub(super) fn emotion_pulse(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        pulse: Pulse<'_>,
    ) -> Result<()> {
        let Pulse {
            at,
            radius,
            kind,
            filter,
            lasts,
            enemies_only,
        } = pulse;
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        if self.emotions.is_empty() || !radius.is_finite() || radius <= 0.0 {
            return Ok(());
        }
        let Some(centre) = self.place_of(tick.state(), ids, source, at) else {
            return Ok(());
        };
        let caster = Self::controller_of(tick.state(), ids, source);
        let source_off_map = tick
            .state()
            .get(ids.body, source)
            .map(|body| self.off_map(tick.state(), ids, body.0.position));
        let reach = radius * radius;
        let mut holders: Vec<Entity> = tick
            .state()
            .store(wiring.emotion_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        holders.sort_unstable_by_key(|entity| entity.index());
        holders.dedup();
        let mut victims = Vec::new();
        for holder in holders {
            if holder == source || self.is_gone(tick.state(), ids, holder) {
                continue;
            }
            let world = tick.state();
            let mut bodies = vec![holder];
            if let Some(horde) = world.get(ids.horde, holder) {
                bodies.extend(horde.members.iter().copied());
            }
            let reached = bodies.into_iter().any(|body| {
                let Some(position) = world.get(ids.body, body).map(|held| held.0.position) else {
                    return false;
                };
                let (dx, dy) = (position.x - centre.x, position.y - centre.y);
                if dx * dx + dy * dy > reach {
                    return false;
                }
                if source_off_map
                    .is_some_and(|off| off != self.off_map(tick.state(), ids, position))
                {
                    return false;
                }
                let Some(record) = world
                    .get(ids.kind, body)
                    .and_then(|held| self.kinds.get(*held))
                    .map(|entry| &entry.record)
                else {
                    return false;
                };
                if enemies_only {
                    if let (Some(caster), Some(other)) =
                        (caster, Self::controller_of(world, ids, body))
                    {
                        if !super::spell_effects::standing(self, caster, Some(other)).2 {
                            return false;
                        }
                    }
                }
                if !filter.passes(&record.kind_of, &record.name) {
                    return false;
                }
                match (caster, Self::controller_of(world, ids, body)) {
                    (Some(caster), Some(other)) => {
                        let allied = self.object_allied(world, source, body);
                        filter.passes_relation(caster == other, allied, !allied)
                    }
                    _ => true,
                }
            });
            if reached {
                victims.push(holder);
            }
        }
        let felt = victims.len();
        for victim in victims {
            self.pulse_emotion(tick, ids, victim, kind, Some(source), lasts)?;
        }
        diag!(
            "emotion: entity {} pulses {} over {radius:.0}; {felt} tracker(s) reached",
            source.index(),
            kind.as_str()
        );
        Ok(())
    }

    pub(super) fn cheer_kills(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        kills: &[(Entity, Entity)],
    ) -> Result<()> {
        if self.emotions.is_empty() {
            return Ok(());
        }
        let filter = crate::defeat::ObjectFilter::parse(&["ANY".to_owned(), "ENEMIES".to_owned()]);
        let lasts = LogicFrames::from_frames(CHEER_PULSE_FRAMES.saturating_mul(TICKS_PER_FRAME));
        for (at, &(victim, _)) in kills.iter().enumerate() {
            let world = tick.state();
            let Some(record) = world
                .get(ids.kind, victim)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| &entry.record)
            else {
                continue;
            };
            let Some(radius) = record.cheer_radius.filter(|radius| *radius > 0.0) else {
                continue;
            };
            let respawns = record
                .declared_modules
                .iter()
                .any(|module| module.name.eq_ignore_ascii_case("RespawnUpdate"));
            if respawns || record.has_kind("PROJECTILE") || record.has_kind("INERT") {
                continue;
            }
            let later = kills.get(at..).unwrap_or_default();
            let horde_holds_another = world
                .get(ids.member, victim)
                .and_then(|member| world.get(ids.horde, member.horde))
                .is_some_and(|horde| {
                    horde
                        .members
                        .iter()
                        .filter(|member| {
                            later.iter().any(|(dying, _)| dying == *member)
                                || world
                                    .get(ids.health, **member)
                                    .is_some_and(|health| !health.0.is_dead())
                        })
                        .count()
                        >= 2
                });
            if horde_holds_another {
                continue;
            }
            self.emotion_pulse(
                tick,
                ids,
                victim,
                Pulse {
                    at: mint_sage_modules::PowerPlace::Source,
                    radius,
                    kind: EmotionType::Cheer,
                    filter: &filter,
                    lasts,
                    enemies_only: false,
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn is_gone(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world.contains(ids.doomed, entity)
            || !world.contains(ids.body, entity)
            || world
                .get(ids.health, entity)
                .is_some_and(|health| health.0.is_dead())
    }

    fn surroundings<'a>(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        living: &'a [(Foe, PlayerId)],
    ) -> Surroundings<'a> {
        let world = tick.state();
        let mut notable = Vec::new();
        let mut threats = Vec::new();
        for (foe, player) in living {
            let Some(record) = world
                .get(ids.kind, foe.entity)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| &entry.record)
            else {
                continue;
            };
            let flags = world.get(ids.shape, foe.entity).map(|shape| shape.kind_of);
            let scary = flags.is_some_and(|kind| kind.contains(mint_sage_asset::KindOf::SCARY));
            let hero = record
                .kind_of
                .iter()
                .any(|kind| kind.eq_ignore_ascii_case("HERO"));
            if scary || hero {
                notable.push(Notable {
                    entity: foe.entity,
                    position: foe.position,
                    radius: foe.radius,
                    player: *player,
                    scary,
                    hero,
                    airborne: self.airborne_category(world, ids, foe.entity).is_some(),
                });
            }
            if let (Some(value), Some(range)) = (record.threat_value, record.vision_range) {
                if value > 0.0 && range > 0.0 {
                    threats.push(ThreatSource {
                        position: foe.position,
                        player: *player,
                        value,
                        range,
                    });
                }
            }
        }
        Surroundings {
            living,
            notable,
            threats,
        }
    }

    fn threat_at(
        &self,
        surroundings: &Surroundings<'_>,
        at: Vec2,
        counts: impl Fn(PlayerId) -> bool,
    ) -> i64 {
        let size = mint_sage_shroud::PARTITION_CELL_SIZE;
        let cell = |value: f32| (value / size).floor();
        let (x, y) = (cell(at.x), cell(at.y));
        let mut total: i64 = 0;
        for source in &surroundings.threats {
            if !counts(source.player) {
                continue;
            }
            let (cx, cy) = (cell(source.position.x), cell(source.position.y));
            let radius_cells = (source.range / size).ceil().max(1.0);
            let (dx, dy) = (x - cx, y - cy);
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > radius_cells {
                continue;
            }
            let share = (1.0 - distance / (radius_cells + 1.0)).clamp(0.0, 1.0);
            #[allow(clippy::cast_possible_truncation)]
            let added = (source.value * share) as i64;
            total = total.saturating_add(added);
        }
        total
    }

    pub(super) fn computer_player(&self, player: PlayerId) -> bool {
        self.ai_seeds.iter().any(|seed| seed.player == player)
    }

    pub(super) fn step_emotions(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        if self.emotions.is_empty() {
            return Ok(());
        }
        let now = tick.frame();
        let mut trackers: Vec<Entity> = tick
            .state()
            .store(wiring.emotion_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        trackers.sort_unstable_by_key(|entity| entity.index());
        trackers.dedup();
        let surroundings = self.surroundings(tick, ids, living);
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for entity in trackers {
            if self.is_gone(tick.state(), ids, entity)
                || self.ai_disabled(tick.state(), ids, entity, now)
            {
                continue;
            }
            let phase = now.get().wrapping_add(u64::from(entity.index()));
            let frame_due = phase % u64::from(TICKS_PER_FRAME) == 0;
            self.advance_emotion_state(tick, ids, entity, frame_due, &mut effects)?;
            if !frame_due {
                continue;
            }
            self.update_tracker(tick, ids, entity, &surroundings, grid, &mut effects)?;
        }
        self.apply_effects(tick, ids, effects)
    }

    fn tracker_of(&self, world: &World, entity: Entity) -> Option<EmotionTrackerUpdate> {
        let wiring = self.modules.as_ref()?;
        world
            .get(wiring.emotion_state, entity)?
            .0
            .first()
            .map(|slot| slot.state.clone())
    }

    fn store_tracker(&self, tick: &mut Tick<'_>, entity: Entity, tracker: EmotionTrackerUpdate) {
        let Some(wiring) = self.modules.as_ref() else {
            return;
        };
        if let Some(column) = tick.state_mut().get_mut(wiring.emotion_state, entity) {
            if let Some(slot) = column.0.first_mut() {
                slot.state = tracker;
            }
        }
    }

    pub(super) fn nuggets_of(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        tracker: &EmotionTrackerUpdate,
    ) -> Cow<'_, [EmotionNuggetDef]> {
        let built = world.get(ids.kind, entity).and_then(|kind| {
            self.modules
                .as_ref()?
                .kind_emotions(*kind)
                .filter(|built| built.declared_by(tracker))
        });
        match built {
            Some(built) => Cow::Borrowed(&built.nuggets),
            None => Cow::Owned(resolve_nuggets(&self.emotions, tracker)),
        }
    }

    fn update_tracker(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        surroundings: &Surroundings<'_>,
        grid: &TargetGrid,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let now = tick.frame();
        if tick
            .state()
            .get(ids.member, entity)
            .is_some_and(|member| tick.state().contains(ids.horde, member.horde))
        {
            return self.deactivate(tick, ids, entity, effects);
        }
        let Some(mut tracker) = self.tracker_of(tick.state(), entity) else {
            return Ok(());
        };
        let mut table = tick
            .state()
            .get(ids.emotion_table, entity)
            .cloned()
            .unwrap_or_default();
        if let Some(forced) = table.forced.as_mut() {
            forced.frames = forced.frames.saturating_sub(1);
        }
        let forced = table.forced.filter(|forced| forced.frames > 0);
        let Some(position) = tick
            .state()
            .get(ids.body, entity)
            .map(|body| body.0.position)
        else {
            return Ok(());
        };
        let owner = Self::controller_of(tick.state(), ids, entity);
        let standing = |other: PlayerId| {
            owner.map_or(Standing::ENEMY, |owner| Standing {
                same_player: owner == other,
                allied: self.alliances.allied(owner, other),
                hostile: self.alliances.hostile(owner, other),
            })
        };
        let threats = Threats {
            enemy: self.threat_at(surroundings, position, |player| standing(player).hostile),
            friend: self.threat_at(surroundings, position, |player| {
                let held = standing(player);
                held.same_player || held.allied
            }),
        };
        let nuggets = self.nuggets_of(tick.state(), ids, entity, &tracker);
        let own_radius = Self::mover_radius(tick, ids, entity);
        let vision = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.vision_range)
            .unwrap_or(0.0);
        let mut point_range = f32::MAX;
        if !tracker.started() {
            tracker.stagger(entity, now);
        }
        if tracker.due(now) && forced.is_none() {
            tracker.scanned(now);
            let taunt = EmotionType::Taunt.code();
            let point = EmotionType::Point.code();
            if self.stands_ground(tick.state(), ids, entity) {
                set_flag(&mut table, taunt, false);
                set_flag(&mut table, point, false);
            } else {
                let found = self.taunt_candidate(
                    tick,
                    ids,
                    &tracker,
                    entity,
                    position,
                    owner,
                    surroundings.living,
                    grid,
                    (&nuggets, &table, threats),
                );
                match found {
                    None => {
                        set_flag(&mut table, taunt, false);
                        set_flag(&mut table, point, false);
                    }
                    Some((candidate, square, pointed)) => {
                        table.point_target = Some(candidate);
                        set_flag(&mut table, point, pointed);
                        set_flag(&mut table, taunt, !pointed);
                        if pointed {
                            point_range = square;
                        }
                    }
                }
            }
        }
        self.store_tracker(tick, entity, tracker.clone());
        let mut scarer = None;
        let mut hero = None;
        if (tracker.hero_scan_distance() > 0.0 || tracker.fear_scan_distance() > 0.0)
            && forced.is_none()
        {
            point_range = point_range.min(vision * vision);
            let mut fear_reach = tracker.fear_scan_distance() * tracker.fear_scan_distance();
            let mut hero_reach = tracker.hero_scan_distance() * tracker.hero_scan_distance();
            let fear_kind = EmotionType::Fear;
            let point_kind = EmotionType::Point;
            for candidate in &surroundings.notable {
                if candidate.entity == entity {
                    continue;
                }
                let held = standing(candidate.player);
                let (dx, dy) = (
                    candidate.position.x - position.x,
                    candidate.position.y - position.y,
                );
                let gap = ((dx * dx + dy * dy).sqrt() - own_radius - candidate.radius).max(0.0);
                let square = gap * gap;
                let seen =
                    owner.is_none_or(|owner| self.sees(tick.state(), ids, owner, candidate.entity));
                let record = tick
                    .state()
                    .get(ids.kind, candidate.entity)
                    .and_then(|kind| self.kinds.get(*kind))
                    .map(|entry| &entry.record);
                let Some(record) = record else { continue };
                if candidate.scary {
                    let same_layer = true;
                    if square < fear_reach
                        && frightens(&tracker, held, true, &record.kind_of, &record.name)
                        && seen
                        && (candidate.airborne || same_layer)
                        && self.any_can(
                            tick,
                            ids,
                            entity,
                            &nuggets,
                            &table,
                            fear_kind,
                            threats,
                            Some(candidate.entity),
                        )
                    {
                        scarer = Some(candidate.entity);
                        fear_reach = square;
                    }
                    if square < point_range
                        && points_at(&tracker, held, &record.kind_of, &record.name)
                        && seen
                        && square <= vision * vision
                        && self.any_can(
                            tick,
                            ids,
                            entity,
                            &nuggets,
                            &table,
                            point_kind,
                            threats,
                            Some(candidate.entity),
                        )
                    {
                        table.point_target = Some(candidate.entity);
                        point_range = square;
                        let pointed = candidate.airborne || same_layer;
                        set_flag(&mut table, EmotionType::Point.code(), pointed);
                        set_flag(&mut table, EmotionType::Taunt.code(), !pointed);
                    }
                }
                if candidate.hero
                    && (held.allied || held.same_player)
                    && square < hero_reach
                    && seen
                {
                    hero = Some(candidate.entity);
                    hero_reach = square;
                }
            }
        }
        table.scarer = scarer;
        table.hero = hero;
        let cheer = EmotionType::HeroCheer.code();
        if hero.is_none() {
            table.hero_latch = false;
        }
        if now > table.hero_cooldown && hero.is_some() && !table.hero_latch {
            set_flag(&mut table, cheer, true);
            table.hero_latch = true;
            table.hero_cooldown = now
                .checked_add(HERO_CHEER_COOLDOWN)
                .unwrap_or(Frame::FOREVER);
        } else {
            set_flag(&mut table, cheer, false);
        }
        let fear = table.pending(EmotionType::Fear.code());
        let pulsed = fear.set && fear.source.is_some() && now <= fear.expiry;
        if !pulsed {
            set_flag(&mut table, EmotionType::Fear.code(), scarer.is_some());
        }
        tick.state_mut().insert(ids.emotion_table, entity, table)?;
        if tracker.quarrel_probability() > 0.0
            && !self.using_ability(tick.state(), ids, entity)
            && self.airborne_category(tick.state(), ids, entity).is_none()
            && tick.rng().next_f32() < tracker.quarrel_probability()
        {
            self.pulse_emotion(tick, ids, entity, EmotionType::Quarrelsome, None, ONE_FRAME)?;
        }
        self.choose_and_run(tick, ids, entity, &tracker, &nuggets, threats, effects)?;
        if let Some(table) = tick.state_mut().get_mut(ids.emotion_table, entity) {
            let horizon = now.checked_add(ONE_FRAME).unwrap_or(Frame::FOREVER);
            for entry in &mut table.pending {
                if entry.expiry != Frame::ZERO && horizon > entry.expiry {
                    entry.set = false;
                    entry.expiry = Frame::ZERO;
                }
            }
            if table.forced.is_some_and(|forced| forced.frames == 0) {
                table.forced = None;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn taunt_candidate(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        tracker: &EmotionTrackerUpdate,
        entity: Entity,
        position: Vec2,
        owner: Option<PlayerId>,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
        (nuggets, table, threats): (&[EmotionNuggetDef], &EmotionTable, Threats),
    ) -> Option<(Entity, f32, bool)> {
        let reach = tracker.taunt_and_point_distance();
        let owner = owner?;
        let world = tick.state();
        if reach <= 0.0 || self.hidden_undetected(world, ids, entity) {
            return None;
        }
        let own_radius = Self::mover_radius(tick, ids, entity);
        let grid_extent = self.passability.grid.extent();
        let enemy = Standing {
            same_player: false,
            allied: false,
            hostile: true,
        };
        let gap_squared = |foe: &Foe| {
            let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
            let gap = ((dx * dx + dy * dy).sqrt() - own_radius - foe.radius).max(0.0);
            gap * gap
        };
        let centre_squared = |foe: &Foe| {
            let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
            dx * dx + dy * dy
        };
        let candidate = |index: u32| -> Option<&(Foe, PlayerId)> {
            let (foe, _) = living.get(usize::try_from(index).ok()?)?;
            if foe.entity == entity
                || !self.object_hostile(world, entity, foe.entity)
                || self.is_gone(world, ids, foe.entity)
            {
                return None;
            }
            let record = &self.kinds.get(*world.get(ids.kind, foe.entity)?)?.record;
            let admitted = !taunt_excluded(tracker, enemy, &record.kind_of, &record.name)
                && self.sees(world, ids, owner, foe.entity)
                && centre_squared(foe) <= reach * reach
                && !self.taunt_wall_blocked(world, ids, grid_extent, position, foe)
                && [EmotionType::Taunt, EmotionType::Point]
                    .into_iter()
                    .any(|kind| {
                        self.any_can(
                            tick,
                            ids,
                            entity,
                            nuggets,
                            table,
                            kind,
                            threats,
                            Some(foe.entity),
                        )
                    });
            admitted
                .then(|| living.get(usize::try_from(index).ok()?))
                .flatten()
        };
        let passing =
            |index: u32| candidate(index).map(|(foe, _)| (centre_squared(foe), foe.entity));
        let nearest =
            super::auto_cast::nearest_by_expanding_scan(position, grid, 0.0, reach, 1, passing)??;
        let at = living
            .binary_search_by_key(&nearest, |(foe, _)| foe.entity)
            .ok()?;
        let (foe, _) = living.get(at)?;
        let record = &self.kinds.get(*world.get(ids.kind, nearest)?)?.record;
        Some((
            nearest,
            gap_squared(foe),
            points_at(tracker, enemy, &record.kind_of, &record.name),
        ))
    }

    fn taunt_wall_blocked(
        &self,
        world: &World,
        ids: Ids,
        extent: mint_movement::GridExtent,
        from: Vec2,
        foe: &Foe,
    ) -> bool {
        let Some(walls) = crate::wall_layer::layer_of(world, ids.wall_layer) else {
            return false;
        };
        let geometry = self.passability.geometry;
        let cell = |point: Vec2| geometry.cell_at(point, 0, extent);
        let tag = |cell: Option<mint_movement::Cell>| cell.map(|cell| walls.tag_at(extent, cell));
        let (from_cell, foe_cell) = (cell(from), cell(foe.position));
        if tag(foe_cell).is_some_and(crate::wall_layer::Layer::is_wall_top) {
            return true;
        }
        let scanner_on_wall = tag(from_cell).is_some_and(crate::wall_layer::Layer::is_wall_top);
        let foe_on_ground = world
            .get(ids.layer, foe.entity)
            .is_none_or(|layer| *layer == crate::wall_layer::Layer::GROUND);
        let (Some(from_cell), Some(foe_cell)) = (from_cell, foe_cell) else {
            return false;
        };
        !scanner_on_wall
            && foe_on_ground
            && !ground_line_clear(
                (i32::from(from_cell.x), i32::from(from_cell.y)),
                (i32::from(foe_cell.x), i32::from(foe_cell.y)),
                |(x, y)| {
                    let cell = u16::try_from(x)
                        .ok()
                        .zip(u16::try_from(y).ok())
                        .map(|(x, y)| mint_movement::Cell::new(x, y))
                        .filter(|cell| extent.index(*cell).is_some());
                    cell.is_none_or(|cell| {
                        walls.tag_at(extent, cell) == crate::wall_layer::Layer::GROUND
                    })
                },
            )
    }

    pub(super) fn using_ability(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world.contains(ids.using_ability, entity)
            || world.get(ids.horde, entity).is_some_and(|horde| {
                horde
                    .members
                    .iter()
                    .any(|member| world.contains(ids.using_ability, *member))
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn any_can(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        nuggets: &[EmotionNuggetDef],
        table: &EmotionTable,
        kind: EmotionType,
        threats: Threats,
        source: Option<Entity>,
    ) -> bool {
        nuggets.iter().enumerate().any(|(index, nugget)| {
            nugget.emotion_type == Some(kind)
                && self.can_activate(tick, ids, entity, index, nugget, table, threats, source)
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn can_activate(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        index: usize,
        nugget: &EmotionNuggetDef,
        table: &EmotionTable,
        threats: Threats,
        source: Option<Entity>,
    ) -> bool {
        let world = tick.state();
        let now = tick.frame();
        if self.using_ability(world, ids, entity) {
            return false;
        }
        let idle = self.regular_state_idle(world, ids, entity);
        if (idle && nugget.ignore_if_unit_idle) || (!idle && nugget.ignore_if_unit_busy) {
            return false;
        }
        if let Some(owner) = Self::controller_of(world, ids, entity) {
            let computer = self.computer_player(owner);
            if (computer && nugget.ignore_if_ai) || (!computer && nugget.ignore_if_human) {
                return false;
            }
        }
        if !threat_admits(
            threats.enemy,
            nugget.only_if_enemy_threat_above,
            nugget.only_if_enemy_threat_below,
        ) || !threat_admits(
            threats.friend,
            nugget.only_if_friend_threat_above,
            nugget.only_if_friend_threat_below,
        ) {
            return false;
        }
        if let Some(clock) = table.clock(index) {
            if now < clock.next_allowed {
                return false;
            }
            if let Some(source) = source {
                if clock
                    .by_object
                    .iter()
                    .any(|(held, until)| *held == source && now < *until)
                {
                    return false;
                }
                let template = world.get(ids.kind, source).map(|kind| kind.0);
                if template.is_some_and(|template| {
                    clock
                        .by_template
                        .iter()
                        .any(|(held, until)| *held == u32::from(template) && now < *until)
                }) {
                    return false;
                }
            }
        }
        match nugget.emotion_type {
            Some(
                kind @ (EmotionType::Fear
                | EmotionType::UncontrollableFear
                | EmotionType::Terror
                | EmotionType::Doom),
            ) => !self.resists(world, ids, entity, kind) && !world.contains(ids.rampaging, entity),
            _ => true,
        }
    }

    fn resists(&self, world: &World, ids: Ids, entity: Entity, kind: EmotionType) -> bool {
        let tested = world
            .get(ids.horde, entity)
            .and_then(|horde| {
                horde
                    .members
                    .iter()
                    .copied()
                    .find(|member| !self.is_gone(world, ids, *member))
            })
            .unwrap_or(entity);
        let bonuses = self.bonuses_of(world, ids, tested);
        let resist = if kind == EmotionType::Terror {
            bonuses.resist_terror
        } else {
            bonuses.resist_fear
        };
        resist.is_finite() && resist >= RESIST_IMMUNE
    }

    pub(super) fn regular_state_idle(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        let routed = |candidate: Entity| {
            world
                .get(ids.path, candidate)
                .is_some_and(|path| path.kind != crate::unit::PathKind::Emotion)
                || world
                    .get(ids.emotion, candidate)
                    .and_then(|emotion| emotion.state.as_ref())
                    .is_some_and(EmotionState::holds_route)
        };
        !(routed(entity)
            || world.contains(ids.attack_order, entity)
            || world.contains(ids.attack_move_order, entity)
            || world
                .get(ids.order_queue, entity)
                .is_some_and(|queue| queue.current.is_some() || !queue.queued.is_empty())
            || world.contains(ids.using_ability, entity))
    }

    fn source_for(table: &EmotionTable, kind: u8) -> Option<Entity> {
        match kind {
            k if k == EmotionType::Taunt.code() || k == EmotionType::Point.code() => {
                table.point_target
            }
            k if k == EmotionType::HeroCheer.code() => table.hero,
            k if k == EmotionType::Fear.code() => table.scarer.or(table.pending(k).source),
            k => table.pending(k).source,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn choose_and_run(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        tracker: &EmotionTrackerUpdate,
        nuggets: &[EmotionNuggetDef],
        threats: Threats,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let now = tick.frame();
        let mut table = tick
            .state()
            .get(ids.emotion_table, entity)
            .cloned()
            .unwrap_or_default();
        let active = tick.state().get(ids.emotion, entity).cloned();
        let level = tick
            .state()
            .get(ids.experience, entity)
            .map_or(0, |experience| u32::from(experience.level));
        let choice = loop {
            let forced = table.forced.filter(|forced| forced.frames > 0);
            let picked = nuggets.iter().enumerate().find_map(|(index, nugget)| {
                let kind = nugget.emotion_type?;
                let code = kind.code();
                let source = match forced {
                    Some(forced) => forced.source,
                    None => Self::source_for(&table, code),
                };
                let is_active = active
                    .as_ref()
                    .is_some_and(|active| usize::from(active.nugget) == index);
                let keeps_running = if nugget.ai_state.is_some() {
                    active.as_ref().is_some_and(|active| active.state.is_some())
                } else {
                    nugget.duration_ms.is_some_and(|ms| ms > 0)
                };
                if is_active
                    && keeps_running
                    && self.can_activate(tick, ids, entity, index, nugget, &table, threats, source)
                {
                    return Some(index);
                }
                match forced {
                    Some(forced) if forced.kind != code => return None,
                    None if !table.pending(code).set => return None,
                    _ => {}
                }
                if forced.is_none()
                    && !tracker.ignore_veterancy()
                    && level >= tracker.immune_to_fear_level()
                    && matches!(
                        kind,
                        EmotionType::Fear | EmotionType::UncontrollableFear | EmotionType::Terror
                    )
                {
                    return None;
                }
                self.can_activate(tick, ids, entity, index, nugget, &table, threats, source)
                    .then_some(index)
            });
            if picked.is_none() && table.forced.is_some_and(|forced| forced.frames > 0) {
                if let Some(forced) = table.forced.as_mut() {
                    forced.frames = 0;
                }
                continue;
            }
            break picked;
        };
        tick.state_mut()
            .insert(ids.emotion_table, entity, table.clone())?;
        let restart = choice
            .and_then(|index| nuggets.get(index))
            .is_some_and(|nugget| {
                let kind = nugget.emotion_type.map(EmotionType::code);
                let pointing = kind == Some(EmotionType::Taunt.code())
                    || kind == Some(EmotionType::Point.code());
                pointing
                    && active.as_ref().is_some_and(|active| {
                        self.horde_or_self(tick.state(), ids, active.cause)
                            != self.horde_or_self(tick.state(), ids, table.point_target)
                    })
            });
        let same = active
            .as_ref()
            .is_some_and(|active| choice == Some(usize::from(active.nugget)));
        if !same || restart {
            self.deactivate(tick, ids, entity, effects)?;
            if let Some(index) = choice {
                let forced = table.forced.filter(|forced| forced.frames > 0);
                let Some(nugget) = nuggets.get(index) else {
                    return Ok(());
                };
                let code = nugget.emotion_type.map_or(0, EmotionType::code);
                let (source, override_frames) = match forced {
                    Some(forced) => (forced.source, Some(forced.frames)),
                    None => (Self::source_for(&table, code), None),
                };
                self.activate(
                    tick,
                    ids,
                    entity,
                    index,
                    nugget,
                    source,
                    override_frames,
                    effects,
                )?;
            }
            return Ok(());
        }
        let Some(active) = active else {
            return Ok(());
        };
        let Some(nugget) = nuggets.get(usize::from(active.nugget)) else {
            return Ok(());
        };
        if self.nugget_update(tick, ids, entity, &active, nugget, effects)? {
            self.deactivate(tick, ids, entity, effects)?;
        }
        let _ = now;
        Ok(())
    }

    fn horde_or_self(&self, world: &World, ids: Ids, entity: Option<Entity>) -> Option<Entity> {
        let entity = entity?;
        Some(
            world
                .get(ids.member, entity)
                .map(|member| member.horde)
                .filter(|horde| world.contains(ids.horde, *horde))
                .unwrap_or(entity),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn activate(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        index: usize,
        nugget: &EmotionNuggetDef,
        cause: Option<Entity>,
        override_frames: Option<u32>,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let now = tick.frame();
        let Some(kind) = nugget.emotion_type else {
            return Ok(());
        };
        let ends = match (
            override_frames.filter(|frames| *frames > 0),
            nugget.duration_ms.filter(|ms| *ms > 0),
        ) {
            (Some(frames), _) => now
                .checked_add(LogicFrames::from_frames(
                    frames.saturating_mul(TICKS_PER_FRAME),
                ))
                .unwrap_or(Frame::FOREVER),
            (None, Some(ms)) => frame_after(now, ms),
            (None, None) => Frame::FOREVER,
        };
        let lock_until = nugget
            .ai_lock_duration_ms
            .filter(|ms| *ms > 0)
            .map_or(Frame::FOREVER, |ms| frame_after(now, ms));
        let emotion = Emotion {
            nugget: u16::try_from(index).unwrap_or(u16::MAX),
            kind: kind.code(),
            cause,
            activated: now,
            ends,
            lock_until,
            prevents_commands: false,
            attribute_applied: false,
            state: None,
        };
        tick.state_mut().insert(ids.emotion, entity, emotion)?;
        if let Some(state) = nugget.ai_state {
            if state != EmotionAiState::AvoidScarer {
                let goal = cause.or_else(|| {
                    matches!(state, EmotionAiState::Idle | EmotionAiState::Quarrel)
                        .then_some(entity)
                });
                if let Some(goal) = goal {
                    self.enter_emotion_state(tick, ids, entity, state, goal, 0.0, effects)?;
                }
            }
        }
        if nugget.prevent_player_commands {
            if let Some(active) = tick.state_mut().get_mut(ids.emotion, entity) {
                active.prevents_commands = true;
            }
        }
        let mut add = nugget.model_conditions.clone();
        if cause.is_some_and(|cause| self.airborne_category(tick.state(), ids, cause).is_some()) {
            add.push("EMOTION_LOOK_TO_SKY".to_owned());
        }
        self.push_conditions_for_horde(
            tick.state(),
            ids,
            entity,
            add,
            nugget.model_conditions_clear.clone(),
            effects,
        );
        if let Some(list) = &nugget.start_fx {
            effects.push((
                entity,
                Effect::PlayFx {
                    list: list.as_str().into(),
                    at: PowerPlace::Source,
                    on_object: true,
                    secondary: cause,
                },
            ));
        }
        diag!(
            "emotion: entity {} feels {} ({}) caused by entity {} at frame {}, ends {}",
            entity.index(),
            kind.as_str(),
            nugget.name,
            cause.map_or(u32::MAX, Entity::index),
            now.get(),
            ends.get()
        );
        Ok(())
    }

    fn nugget_update(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        active: &Emotion,
        nugget: &EmotionNuggetDef,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<bool> {
        let now = tick.frame();
        let point_lost = active.kind == EmotionType::Point.code()
            && active
                .cause
                .is_none_or(|cause| self.is_gone(tick.state(), ids, cause));
        if now >= active.ends || point_lost {
            return Ok(true);
        }
        if !active.attribute_applied {
            if let Some(list) = &nugget.attribute_modifier {
                let start = frame_after(
                    active.activated,
                    nugget.attribute_start_delay_ms.unwrap_or(0),
                );
                if now >= start {
                    let until = if nugget.attribute_modifier_while_active {
                        Frame::FOREVER
                    } else {
                        frame_after(now, nugget.attribute_duration_ms.unwrap_or(0))
                    };
                    self.give_emotion_modifier(tick, ids, entity, list, until)?;
                    if let Some(held) = tick.state_mut().get_mut(ids.emotion, entity) {
                        held.attribute_applied = true;
                    }
                }
            }
        }
        if now >= active.lock_until {
            if let Some(state) = tick
                .state_mut()
                .get_mut(ids.emotion, entity)
                .and_then(|held| held.state.as_mut())
            {
                state.unlock();
            }
        }
        if let Some(list) = &nugget.update_fx {
            effects.push((
                entity,
                Effect::PlayFx {
                    list: list.as_str().into(),
                    at: PowerPlace::Source,
                    on_object: true,
                    secondary: active.cause,
                },
            ));
        }
        Ok(false)
    }

    fn give_emotion_modifier(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        list: &str,
        until: Frame,
    ) -> Result<()> {
        let Some(id) = self.combat.modifiers.id(list) else {
            diag!("emotion: ModifierList {list} is not declared");
            return Ok(());
        };
        self.give_timed_list(tick, ids, entity, id, until)?;
        Ok(())
    }

    pub(super) fn deactivate(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let Some(active) = tick.state().get(ids.emotion, entity).cloned() else {
            return Ok(());
        };
        let now = tick.frame();
        self.leave_emotion_state(tick, ids, entity, effects)?;
        tick.state_mut().remove(ids.emotion, entity);
        let Some(tracker) = self.tracker_of(tick.state(), entity) else {
            return Ok(());
        };
        let nuggets = self.nuggets_of(tick.state(), ids, entity, &tracker);
        let index = usize::from(active.nugget);
        let Some(nugget) = nuggets.get(index) else {
            return Ok(());
        };
        let template = active
            .cause
            .and_then(|cause| tick.state().get(ids.kind, cause))
            .map(|kind| u32::from(kind.0));
        let mut table = tick
            .state()
            .get(ids.emotion_table, entity)
            .cloned()
            .unwrap_or_default();
        if let Some(clock) = table.clock_mut(index) {
            clock.by_object.retain(|(_, until)| now < *until);
            clock.by_template.retain(|(_, until)| now < *until);
            if let Some(ms) = nugget.inactive_duration_ms {
                clock.next_allowed = frame_after(now, ms);
            }
            if let (Some(ms), Some(cause)) = (nugget.inactive_duration_same_object_ms, active.cause)
            {
                set_until(&mut clock.by_object, cause, frame_after(now, ms));
            }
            if let (Some(ms), Some(template)) = (nugget.inactive_duration_same_type_ms, template) {
                set_until(&mut clock.by_template, template, frame_after(now, ms));
            }
        }
        tick.state_mut().insert(ids.emotion_table, entity, table)?;
        let mut remove = nugget.model_conditions.clone();
        remove.push("EMOTION_LOOK_TO_SKY".to_owned());
        remove.extend(nugget.model_conditions_clear_on_exit.iter().cloned());
        self.push_conditions_for_horde(
            tick.state(),
            ids,
            entity,
            nugget.model_conditions_set_on_exit.clone(),
            remove,
            effects,
        );
        if let Some(list) = &nugget.end_fx {
            effects.push((
                entity,
                Effect::PlayFx {
                    list: list.as_str().into(),
                    at: PowerPlace::Source,
                    on_object: true,
                    secondary: active.cause,
                },
            ));
        }
        if nugget.attribute_modifier_while_active && active.attribute_applied {
            if let Some(id) = nugget
                .attribute_modifier
                .as_deref()
                .and_then(|list| self.combat.modifiers.id(list))
            {
                self.remove_timed_list(tick, ids, entity, id)?;
            }
        }
        diag!(
            "emotion: entity {} stops feeling {} at frame {}",
            entity.index(),
            nugget.name,
            now.get()
        );
        Ok(())
    }

    pub(super) fn push_conditions_for_horde(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        add: Vec<String>,
        remove: Vec<String>,
        effects: &mut Vec<(Entity, Effect)>,
    ) {
        if add.is_empty() && remove.is_empty() {
            return;
        }
        let mut targets = vec![entity];
        if let Some(horde) = world.get(ids.horde, entity) {
            targets.extend(horde.members.iter().copied());
        }
        for target in targets {
            effects.push((
                target,
                Effect::ModelConditions {
                    add: mint_sage_modules::effects::names(add.clone()),
                    remove: mint_sage_modules::effects::names(remove.clone()),
                },
            ));
        }
    }
}

pub(super) fn ground_line_clear(
    from: (i32, i32),
    to: (i32, i32),
    ground: impl Fn((i32, i32)) -> bool,
) -> bool {
    let (dx, dy) = (to.0.saturating_sub(from.0), to.1.saturating_sub(from.1));
    let steps = dx.abs().max(dy.abs());
    (0..=steps).all(|step| {
        let along = |delta: i32, origin: i32| {
            if steps == 0 {
                origin
            } else {
                let scaled = i64::from(delta) * i64::from(step) * 2 + i64::from(steps);
                let rounded = scaled.div_euclid(i64::from(steps) * 2);
                origin.saturating_add(i32::try_from(rounded).unwrap_or(0))
            }
        };
        ground((along(dx, from.0), along(dy, from.1)))
    })
}

fn set_flag(table: &mut EmotionTable, kind: u8, on: bool) {
    if let Some(entry) = table.pending_mut(kind) {
        entry.set = on;
    }
}

fn threat_admits(threat: i64, above: Option<f32>, below: Option<f32>) -> bool {
    #[allow(clippy::cast_possible_truncation)]
    let whole = |value: f32| value as i64;
    above.is_none_or(|limit| threat > whole(limit))
        && below.is_none_or(|limit| threat < whole(limit))
}

fn set_until<K: Ord + Copy>(list: &mut Vec<(K, Frame)>, key: K, until: Frame) {
    match list.binary_search_by_key(&key, |(held, _)| *held) {
        Ok(at) => {
            if let Some(entry) = list.get_mut(at) {
                entry.1 = entry.1.max(until);
            }
        }
        Err(at) => list.insert(at, (key, until)),
    }
}
