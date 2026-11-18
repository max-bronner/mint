use std::collections::{BTreeMap, VecDeque};

use mint_sage_audio::Eva;
use mint_sage_ini::eva::{EvaDefs, ScoredKillEvaAnnouncerDef};
use mint_sage_ini::object::ObjectIndex;
use mint_sage_modules::object_filter::ObjectFilter;
use mint_sim::{Entity, PlayerId};

use crate::combat_facts::CombatFacts;
use crate::game_data::RETAIL_LOGIC_HZ;

#[derive(Debug, Clone, Copy, Default)]
struct KindEva {
    die_owner: Option<usize>,
    die_ally: Option<usize>,
    die_enemy: Option<usize>,
    damaged: Option<usize>,
    damaged_shrouded: Option<usize>,
    damaged_by_fire: Option<usize>,
    second: Option<usize>,
    second_range: f32,
    second_timeout_frames: u32,
    last_member_death: Option<usize>,
}

#[derive(Debug, Clone)]
struct Tracker {
    scorer: Option<PlayerId>,
    kills: VecDeque<(u32, [f32; 3])>,
}

#[derive(Debug, Clone)]
struct Counter {
    def: ScoredKillEvaAnnouncerDef,
    event: Option<usize>,
    filter: ObjectFilter,
    window_frames: u32,
    trackers: Vec<Tracker>,
}

pub(crate) struct AnnouncerFacts<'a> {
    pub(crate) local: PlayerId,
    pub(crate) units: &'a [crate::render_state::UnitView],
    pub(crate) kinds: &'a crate::templates::KindTable,
    pub(crate) rules: &'a crate::rules::RotwkRules,
    pub(crate) world: &'a mint_sim::World,
    pub(crate) frame: mint_sim::Frame,
    pub(crate) defeats: &'a [PlayerId],
}

pub(crate) struct Announcer {
    eva: Eva,
    kinds: BTreeMap<u16, KindEva>,
    second_until: BTreeMap<u32, u32>,
    attach_died: BTreeMap<u32, usize>,
    counters: Vec<Counter>,
    logical_fire: Option<mint_sage_combat::DamageTypeId>,
    excluded: Vec<mint_sage_combat::DamageTypeId>,
    last_rank: Option<u16>,
    now: u32,
}

fn retail_frame(frame: mint_sim::Frame) -> u32 {
    let per = u64::from(mint_sim::SIMULATION_HZ / RETAIL_LOGIC_HZ).max(1);
    u32::try_from(frame.get() / per).unwrap_or(u32::MAX)
}

fn duration_frames(ms: u32) -> u32 {
    u32::try_from((u64::from(ms) * u64::from(RETAIL_LOGIC_HZ)).div_ceil(1000)).unwrap_or(u32::MAX)
}

impl Announcer {
    pub(crate) fn new(
        defs: &EvaDefs,
        index: Option<&ObjectIndex>,
        templates: &[String],
        rules: &crate::rules::RotwkRules,
    ) -> Self {
        let eva = Eva::new(defs, RETAIL_LOGIC_HZ);
        let mut kinds = BTreeMap::new();
        if let Some(index) = index {
            for (slot, name) in templates.iter().enumerate() {
                let Ok(kind) = u16::try_from(slot) else {
                    continue;
                };
                let event = |field: &str| index.sound(name, field).and_then(|e| eva.id(e));
                let number = |field: &str| {
                    index
                        .sound(name, field)
                        .and_then(mint_sage_ini::value::parse_float)
                };
                let entry = KindEva {
                    die_owner: event("EvaEventDieOwner"),
                    die_ally: event("EvaEventDieAlly"),
                    die_enemy: event("EvaEventDieEnemy"),
                    damaged: event("EvaEventDamagedOwner"),
                    damaged_shrouded: event("EvaEventDamagedFromShroudedSourceOwner"),
                    damaged_by_fire: event("EvaEventDamagedByFireOwner"),
                    second: event("EvaEventSecondDamageFarFromFirstOwner"),
                    second_range: number("EvaEventSecondDamageFarFromFirstScanRange")
                        .unwrap_or(0.0),
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    second_timeout_frames: duration_frames(
                        number("EvaEventSecondDamageFarFromFirstTimeoutMS")
                            .unwrap_or(0.0)
                            .clamp(0.0, u32::MAX as f32) as u32,
                    ),
                    last_member_death: rules
                        .formation_of(crate::unit::Kind(kind))
                        .and_then(|table| table.settings().eva_event_last_member_death.as_deref())
                        .and_then(|name| eva.id(name)),
                };
                if entry.die_owner.is_some()
                    || entry.die_ally.is_some()
                    || entry.die_enemy.is_some()
                    || entry.damaged.is_some()
                    || entry.damaged_shrouded.is_some()
                    || entry.damaged_by_fire.is_some()
                    || entry.last_member_death.is_some()
                {
                    kinds.insert(kind, entry);
                }
            }
        }
        let counters = defs
            .announcers
            .iter()
            .map(|def| Counter {
                event: eva.id(&def.event),
                filter: ObjectFilter::parse(&def.object_filter),
                window_frames: duration_frames(def.maximum_time_ms),
                trackers: Vec::new(),
                def: def.clone(),
            })
            .collect();
        let excluded = ["HEALING", "PENALTY"]
            .iter()
            .filter_map(|name| rules.damage_type_named(name))
            .collect();
        Self {
            eva,
            kinds,
            second_until: BTreeMap::new(),
            attach_died: BTreeMap::new(),
            counters,
            logical_fire: rules.damage_type_named("LOGICAL_FIRE"),
            excluded,
            last_rank: None,
            now: 0,
        }
    }

    pub(crate) fn readout(&self) -> String {
        format!(
            "{} event(s), {} template(s) with announcer fields, {} kill counter(s)",
            self.eva.len(),
            self.kinds.len(),
            self.counters.len()
        )
    }

    pub(crate) fn last_played(&self, event: &str) -> Option<Option<(u32, [f32; 2])>> {
        let id = self.eva.id(event)?;
        Some(
            self.eva
                .last_played(id)
                .map(|(frame, at)| (self.now.saturating_sub(frame), [at[0], at[1]])),
        )
    }

    pub(crate) fn report_voice(&mut self, event: &str, frame: mint_sim::Frame, at: [f32; 3]) {
        if let Some(id) = self.eva.id(event) {
            crate::diag::diag!("eva: voice reports {event}");
            self.eva.report(id, retail_frame(frame), Some(at));
        }
    }

    pub(crate) fn observe(
        &mut self,
        facts: &AnnouncerFacts<'_>,
        combat: Option<&CombatFacts>,
        play: &mut dyn FnMut(&mint_sage_audio::EvaLine) -> Option<u32>,
    ) -> Option<[f32; 2]> {
        let now = retail_frame(facts.frame);
        self.now = now;
        if let Some(combat) = combat {
            for struck in combat.struck(facts.frame) {
                self.on_struck(facts, now, struck);
            }
            for &(entity, faded) in combat.deaths(facts.frame) {
                if !faded {
                    self.on_death(facts, now, entity);
                }
            }
            for &(victim, killer) in combat.kills(facts.frame) {
                self.on_kill(facts, now, victim, killer);
            }
            for &(entity, parent_owner) in combat.attaches(facts.frame) {
                self.on_attach(facts, now, entity, parent_owner);
            }
            for &entity in combat.attach_losses(facts.frame) {
                self.on_attach_loss(facts, now, entity);
            }
            for &entity in combat.last_member_deaths(facts.frame) {
                self.on_last_member_death(facts, now, entity);
            }
            for &(detected, detector) in combat.detections(facts.frame) {
                self.on_detection(facts, now, detected, detector);
            }
            for &(climber, wall) in combat.wall_climbs(facts.frame) {
                self.on_wall_climb(facts, now, climber, wall);
            }
        }
        for &defeated in facts.defeats {
            self.on_defeat(facts, now, defeated);
        }
        self.check_rank_up(facts, now);
        self.count_kills(now);
        let side = facts
            .rules
            .player_sides()
            .iter()
            .find(|(player, _)| *player == facts.local)
            .map_or("", |(_, side)| side.as_str())
            .to_string();
        let home = home_base(facts);
        let elapsed = 1000.0 / mint_sim::SIMULATION_HZ as f32;
        let line = self.eva.update(now, elapsed, &side, home, play)?;
        crate::diag::diag!(
            "eva: {} speaks {} at ({:.0}, {:.0})",
            line.event,
            line.sound,
            line.at[0],
            line.at[1]
        );
        let jumps = self
            .eva
            .id(&line.event)
            .and_then(|id| self.eva.def(id))
            .is_some_and(|def| def.count_as_jump_to_location);
        (jumps && line.has_place()).then(|| [line.at[0], line.at[1]])
    }

    fn place(
        facts: &AnnouncerFacts<'_>,
        entity: Entity,
    ) -> Option<([f32; 3], u16, bool, Option<PlayerId>)> {
        facts
            .rules
            .voice_target(facts.world, entity, facts.local)
            .map(|(at, kind, visible, owner)| ([at.x, at.y, 0.0], kind, visible, owner))
    }

    fn on_struck(
        &mut self,
        facts: &AnnouncerFacts<'_>,
        now: u32,
        struck: &crate::combat_facts::Struck,
    ) {
        if self.excluded.contains(&struck.damage_type) {
            return;
        }
        let Some((at, kind, _, Some(owner))) = Self::place(facts, struck.victim) else {
            return;
        };
        if owner != facts.local {
            return;
        }
        let attacker = Self::place(facts, struck.attacker);
        if attacker.is_some_and(|(_, _, _, by)| by == Some(owner)) {
            return;
        }
        let Some(k) = self.kinds.get(&kind).copied() else {
            return;
        };
        let (event, second) =
            if Some(struck.damage_type) == self.logical_fire && k.damaged_by_fire.is_some() {
                (k.damaged_by_fire, None)
            } else if attacker.is_some_and(|(_, _, visible, _)| !visible)
                && k.damaged_shrouded.is_some()
            {
                (k.damaged_shrouded, k.second)
            } else {
                (k.damaged, k.second)
            };
        let Some(event) = event else {
            return;
        };
        let held = self
            .second_until
            .get(&struck.victim.index())
            .is_some_and(|until| *until > now);
        let busy = self.eva.is_blocked(event) || self.eva.is_about_to_play(event);
        let reported = match second {
            Some(second) if !held && busy => second,
            _ => event,
        };
        self.eva.report(reported, now, Some(at));
        if k.second_range <= 0.0 || k.damaged.is_none() {
            self.hold_second(struck.victim.index(), now, k.second_timeout_frames);
            return;
        }
        let range_sq = k.second_range * k.second_range;
        let near: Vec<(u32, u32)> = facts
            .units
            .iter()
            .filter(|u| u.owner == owner)
            .filter(|u| {
                let (dx, dy) = (u.position.x - at[0], u.position.y - at[1]);
                dx * dx + dy * dy <= range_sq
            })
            .filter_map(|u| {
                let other = u.kind.and_then(|kind| self.kinds.get(&kind))?;
                (other.damaged == k.damaged)
                    .then_some((u.entity.index(), other.second_timeout_frames))
            })
            .collect();
        for (entity, timeout) in near {
            self.hold_second(entity, now, timeout);
        }
    }

    fn hold_second(&mut self, entity: u32, now: u32, timeout: u32) {
        let until = now.saturating_add(timeout);
        let held = self.second_until.entry(entity).or_insert(0);
        *held = (*held).max(until);
    }

    fn on_death(&mut self, facts: &AnnouncerFacts<'_>, now: u32, entity: Entity) {
        self.second_until.remove(&entity.index());
        let Some((at, kind, _, owner)) = Self::place(facts, entity) else {
            return;
        };
        let Some(k) = self.kinds.get(&kind).copied() else {
            return;
        };
        let event = match owner {
            Some(owner) if owner == facts.local => k.die_owner,
            Some(owner) if facts.rules.allied(facts.local, owner) => k.die_ally,
            _ => k.die_enemy,
        };
        if let Some(event) = event {
            self.eva.report(event, now, Some(at));
        }
    }

    fn check_rank_up(&mut self, facts: &AnnouncerFacts<'_>, now: u32) {
        let Some(ids) = facts.rules.ids() else {
            return;
        };
        let Some(state) = facts.rules.science_of(facts.world, ids, facts.local) else {
            return;
        };
        let rank = state.rank;
        if self.last_rank.is_some_and(|last| rank > last) {
            if let Some(event) = self.eva.id("GeneralLevelUp") {
                self.eva.report(event, now, None);
            }
        }
        self.last_rank = Some(rank);
    }

    fn on_last_member_death(&mut self, facts: &AnnouncerFacts<'_>, now: u32, entity: Entity) {
        let Some((at, kind, ..)) = Self::place(facts, entity) else {
            return;
        };
        let Some(event) = self.kinds.get(&kind).and_then(|k| k.last_member_death) else {
            return;
        };
        self.eva.report(event, now, Some(at));
    }

    fn on_detection(
        &mut self,
        facts: &AnnouncerFacts<'_>,
        now: u32,
        detected: Entity,
        detector: Entity,
    ) {
        let Some((detected_at, _, _, owner)) = Self::place(facts, detected) else {
            return;
        };
        let (owner_name, ally_name, enemy_name) =
            facts.rules.detected_event_names(facts.world, detected);
        let (event, at) = match owner {
            Some(owner) if owner == facts.local => (owner_name, detected_at),
            Some(owner) if facts.rules.allied(facts.local, owner) => (ally_name, detected_at),
            _ => {
                let at = Self::place(facts, detector)
                    .map_or(detected_at, |(detector_at, ..)| detector_at);
                (enemy_name, at)
            }
        };
        if let Some(event) = event.and_then(|name| self.eva.id(name)) {
            self.eva.report(event, now, Some(at));
        }
    }

    fn on_wall_climb(
        &mut self,
        facts: &AnnouncerFacts<'_>,
        now: u32,
        climber: Entity,
        wall: Entity,
    ) {
        let Some((at, _, _, Some(wall_owner))) = Self::place(facts, wall) else {
            return;
        };
        if wall_owner != facts.local {
            return;
        }
        let Some((_, _, _, Some(climber_owner))) = Self::place(facts, climber) else {
            return;
        };
        if !facts.rules.hostile(facts.local, climber_owner) {
            return;
        }
        if let Some(event) = self.eva.id("WallsBeingClimbed") {
            self.eva.report(event, now, Some(at));
        }
    }

    fn on_defeat(&mut self, facts: &AnnouncerFacts<'_>, now: u32, defeated: PlayerId) {
        if defeated == facts.local {
            crate::diag::diag!("eva: GUI:YouHaveBeenDefeated");
            return;
        }
        let name = facts
            .rules
            .player_sides()
            .iter()
            .find(|(player, _)| *player == defeated)
            .map_or("", |(_, side)| side.as_str());
        crate::diag::diag!("eva: GUI:PlayerHasBeenDefeated {name}");
        let event = if facts.rules.allied(facts.local, defeated) {
            "AllyDefeated"
        } else {
            "EnemyDefeated"
        };
        if let Some(event) = self.eva.id(event) {
            self.eva.report(event, now, None);
        }
    }

    fn on_attach(
        &mut self,
        facts: &AnnouncerFacts<'_>,
        now: u32,
        entity: Entity,
        parent_owner: Option<PlayerId>,
    ) {
        let Some(module) = facts.rules.attach_view(facts.world, entity) else {
            return;
        };
        let (attachment, died) = match parent_owner {
            Some(owner) if owner == facts.local => {
                (module.owner_attachment_event(), module.owner_died_event())
            }
            Some(owner) if facts.rules.allied(facts.local, owner) => {
                (module.ally_attachment_event(), module.ally_died_event())
            }
            _ => (module.enemy_attachment_event(), module.enemy_died_event()),
        };
        if let Some(event) = died.and_then(|name| self.eva.id(name)) {
            self.attach_died.insert(entity.index(), event);
        } else {
            self.attach_died.remove(&entity.index());
        }
        let Some(event) = attachment.and_then(|name| self.eva.id(name)) else {
            return;
        };
        let Some((at, ..)) = Self::place(facts, entity) else {
            return;
        };
        self.eva.report(event, now, Some(at));
    }

    fn on_attach_loss(&mut self, facts: &AnnouncerFacts<'_>, now: u32, entity: Entity) {
        let Some(event) = self.attach_died.remove(&entity.index()) else {
            return;
        };
        if let Some((at, ..)) = Self::place(facts, entity) {
            self.eva.report(event, now, Some(at));
        }
    }

    fn on_kill(&mut self, facts: &AnnouncerFacts<'_>, now: u32, victim: Entity, killer: Entity) {
        let Some((at, kind, _, victim_owner)) = Self::place(facts, victim) else {
            return;
        };
        let scorer = Self::place(facts, killer).and_then(|(_, _, _, owner)| owner);
        let Some(entry) = facts.kinds.get(crate::unit::Kind(kind)) else {
            return;
        };
        for counter in &mut self.counters {
            if counter.def.only_kills_against_local_player && victim_owner != Some(facts.local) {
                continue;
            }
            if !counter
                .filter
                .passes(&entry.record.kind_of, &entry.record.name)
            {
                continue;
            }
            if counter.def.only_kills_by_local_player && scorer != Some(facts.local) {
                continue;
            }
            let tracker = match counter.trackers.iter_mut().find(|t| t.scorer == scorer) {
                Some(tracker) => tracker,
                None => {
                    let at = counter.trackers.partition_point(|t| t.scorer < scorer);
                    counter.trackers.insert(
                        at,
                        Tracker {
                            scorer,
                            kills: VecDeque::new(),
                        },
                    );
                    match counter.trackers.get_mut(at) {
                        Some(tracker) => tracker,
                        None => continue,
                    }
                }
            };
            tracker.kills.push_back((now, at));
        }
    }

    fn count_kills(&mut self, now: u32) {
        for counter in &mut self.counters {
            let mut total = 0usize;
            let mut place = None;
            for tracker in &mut counter.trackers {
                while tracker
                    .kills
                    .front()
                    .is_some_and(|(at, _)| at.saturating_add(counter.window_frames) < now)
                {
                    tracker.kills.pop_front();
                }
            }
            let minimum = usize::try_from(counter.def.minimum_count).unwrap_or(usize::MAX);
            for tracker in &counter.trackers {
                if total >= minimum {
                    break;
                }
                total = total.saturating_add(tracker.kills.len());
                if let Some((_, at)) = tracker.kills.back() {
                    place = Some(*at);
                }
            }
            if total >= minimum {
                if let Some(event) = counter.event {
                    self.eva.report(event, now, place);
                }
            }
        }
    }
}

fn home_base(facts: &AnnouncerFacts<'_>) -> Option<[f32; 3]> {
    let mut best: Option<(&crate::render_state::UnitView, bool, f32)> = None;
    for unit in facts.units {
        if unit.owner != facts.local || unit.dying {
            continue;
        }
        let Some(entry) = unit
            .kind
            .and_then(|k| facts.kinds.get(crate::unit::Kind(k)))
        else {
            continue;
        };
        if !entry.record.has_kind("STRUCTURE") {
            continue;
        }
        let centre = entry.record.has_kind("COMMANDCENTER");
        let cost = entry.record.build_cost.unwrap_or(0.0);
        let better = match best {
            None => true,
            Some((held, held_centre, held_cost)) => match (centre, held_centre) {
                (true, false) => true,
                (false, true) => false,
                (true, true) => unit.entity.index() < held.entity.index(),
                (false, false) => cost > held_cost,
            },
        };
        if better {
            best = Some((unit, centre, cost));
        }
    }
    best.map(|(unit, _, _)| [unit.position.x, unit.position.y, unit.ground.max(0.0)])
}

#[cfg(test)]
mod tests;
