use crate::render_state::UnitView;
use mint_sim::PlayerId;

pub(crate) const RADAR_EVENT_SLOTS: usize = 64;

const LIFE_SECONDS: f32 = 4.0;

const FADE_BEFORE_DEATH_SECONDS: f32 = 0.5;

const SUPPRESS_DISTANCE_SQ: f32 = 600.0 * 600.0;

const SUPPRESS_SECONDS: f32 = 10.0;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RadarEventType {
    Information,
    Construction,
    Upgrade,
    UnderAttack,
    Infiltration,
    Banner,
    BeaconPulse,
    BattlePlan,
    StealthDiscovered,
    StealthNeutralized,
    Fake,
}

impl RadarEventType {
    pub(crate) const fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::Information,
            1 => Self::Construction,
            2 => Self::Upgrade,
            3 => Self::UnderAttack,
            4 => Self::Infiltration,
            5 => Self::Banner,
            6 => Self::BeaconPulse,
            7 => Self::BattlePlan,
            8 => Self::StealthDiscovered,
            9 => Self::StealthNeutralized,
            10 => Self::Fake,
            _ => return None,
        })
    }

    pub(crate) const fn ping_clip(self) -> Option<&'static str> {
        match self {
            Self::Information => Some("PingInformation"),
            Self::Construction => Some("PingGeneric"),
            Self::Upgrade => Some("PingUpgrade"),
            Self::UnderAttack => Some("PingAttack"),
            Self::BeaconPulse => Some("PingBeacon"),
            Self::Infiltration
            | Self::Banner
            | Self::BattlePlan
            | Self::StealthDiscovered
            | Self::StealthNeutralized
            | Self::Fake => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RadarEvent {
    pub(crate) serial: u32,
    pub(crate) kind: RadarEventType,
    pub(crate) at: (f32, f32),
    create_time: f32,
    die_time: f32,
    fade_time: f32,
    sound_played: bool,
}

impl RadarEvent {
    fn alive(&self, now: f32) -> bool {
        now < self.die_time
    }

    pub(crate) fn fading(&self, now: f32) -> bool {
        now >= self.fade_time
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RadarEventRing {
    slots: [Option<RadarEvent>; RADAR_EVENT_SLOTS],
    next: usize,
    next_serial: u32,
}

impl Default for RadarEventRing {
    fn default() -> Self {
        Self {
            slots: [None; RADAR_EVENT_SLOTS],
            next: 0,
            next_serial: 0,
        }
    }
}

impl RadarEventRing {
    pub(crate) fn create(&mut self, kind: RadarEventType, at: (f32, f32), now: f32) {
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1);
        if let Some(slot) = self.slots.get_mut(self.next) {
            *slot = Some(RadarEvent {
                serial,
                kind,
                at,
                create_time: now,
                die_time: now + LIFE_SECONDS,
                fade_time: now + LIFE_SECONDS - FADE_BEFORE_DEATH_SECONDS,
                sound_played: false,
            });
        }
        self.next = (self.next + 1) % RADAR_EVENT_SLOTS;
    }

    pub(crate) fn try_event(&mut self, kind: RadarEventType, at: (f32, f32), now: f32) -> bool {
        let suppressed = self.slots.iter().flatten().any(|slot| {
            slot.kind == kind
                && now - slot.create_time < SUPPRESS_SECONDS
                && distance_sq(slot.at, at) <= SUPPRESS_DISTANCE_SQ
        });
        if suppressed {
            return false;
        }
        self.create(kind, at, now);
        true
    }

    pub(crate) fn drawable(&self, now: f32) -> impl Iterator<Item = &RadarEvent> {
        self.slots
            .iter()
            .flatten()
            .filter(move |slot| slot.alive(now) && !matches!(slot.kind, RadarEventType::Fake))
    }

    pub(crate) fn take_unsounded(&mut self, now: f32) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        for slot in self.slots.iter_mut().flatten() {
            if slot.alive(now)
                && !slot.sound_played
                && !matches!(slot.kind, RadarEventType::BeaconPulse)
            {
                slot.sound_played = true;
                out.push(slot.at);
            }
        }
        out
    }
}

fn distance_sq(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

pub(crate) fn under_attack_label(has_kind: impl Fn(&str) -> bool) -> &'static str {
    if has_kind("INFANTRY") || has_kind("CAVALRY") {
        if has_kind("HARVESTER") {
            "RADAR:HarvesterUnderAttack"
        } else {
            "RADAR:UnitUnderAttack"
        }
    } else if has_kind("STRUCTURE") || has_kind("MP_COUNT_FOR_VICTORY") {
        "RADAR:StructureUnderAttack"
    } else {
        "RADAR:UnderAttack"
    }
}

pub(crate) fn observe_under_attack(
    ring: &mut RadarEventRing,
    units: &[UnitView],
    kinds: &crate::templates::KindTable,
    struck: &[crate::combat_facts::Struck],
    local: PlayerId,
    now: f32,
) {
    for hit in struck {
        let Some(victim) = units.iter().find(|u| u.entity == hit.victim) else {
            continue;
        };
        if victim.owner != local {
            continue;
        }
        let at = (victim.position.x, victim.position.y);
        if ring.try_event(RadarEventType::UnderAttack, at, now) {
            let record = victim
                .kind
                .and_then(|kind| kinds.get(crate::unit::Kind(kind)))
                .map(|entry| &entry.record);
            let label = under_attack_label(|token| record.is_some_and(|r| r.has_kind(token)));
            println!("{label}: under attack at ({:.1}, {:.1})", at.0, at.1);
        }
    }
}

#[cfg(test)]
mod tests;
