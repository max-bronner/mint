use std::collections::BTreeMap;

use mint_sage_audio::{Outcome, Overrides, Reach};
use mint_sage_ini::audio::AudioPriority;
use mint_sage_map::{Dictionary, Value};
use mint_sage_scene::Conditions;

use crate::render_state::UnitView;

pub(crate) const SOUND_AMBIENT: &str = "SoundAmbient";
pub(crate) const SOUND_AMBIENT_DAMAGED: &str = "SoundAmbientDamaged";
pub(crate) const SOUND_AMBIENT_REALLY_DAMAGED: &str = "SoundAmbientReallyDamaged";
pub(crate) const SOUND_AMBIENT_RUBBLE: &str = "SoundAmbientRubble";
pub(crate) const SOUND_AMBIENT_BATTLE: &str = "SoundAmbientBattle";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rung {
    Pristine,
    Damaged,
    ReallyDamaged,
    Rubble,
}

impl Rung {
    pub(crate) fn of(unit: &UnitView) -> Self {
        if unit.dying {
            Self::Rubble
        } else if unit.conditions.contains(Conditions::REALLYDAMAGED) {
            Self::ReallyDamaged
        } else if unit.conditions.contains(Conditions::DAMAGED) {
            Self::Damaged
        } else {
            Self::Pristine
        }
    }
}

pub(crate) fn fields(rung: Rung, under_construction: bool) -> &'static [&'static str] {
    if under_construction && rung != Rung::Rubble {
        return &[SOUND_AMBIENT];
    }
    match rung {
        Rung::Pristine => &[SOUND_AMBIENT],
        Rung::Damaged => &[SOUND_AMBIENT_DAMAGED, SOUND_AMBIENT],
        Rung::ReallyDamaged => &[SOUND_AMBIENT_REALLY_DAMAGED, SOUND_AMBIENT],
        Rung::Rubble => &[SOUND_AMBIENT_RUBBLE],
    }
}

pub(crate) fn under_construction(conditions: Conditions) -> bool {
    conditions.contains(Conditions::AWAITING_CONSTRUCTION)
        || conditions.contains(Conditions::ACTIVELY_BEING_CONSTRUCTED)
}

pub(crate) fn battle(conditions: Conditions) -> bool {
    conditions.contains(Conditions::ENGAGED)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EventFacts {
    pub(crate) permanent: bool,
    pub(crate) stream: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Placed {
    pub(crate) event: String,
    pub(crate) overrides: Overrides,
    pub(crate) enabled: bool,
}

const KEY_AMBIENT: &str = "objectSoundAmbient";
const KEY_CUSTOMIZED: &str = "objectSoundAmbientCustomized";
const KEY_ENABLED: &str = "objectSoundAmbientEnabled";
const KEY_LOOPING: &str = "objectSoundAmbientLooping";
const KEY_MIN_VOLUME: &str = "objectSoundAmbientMinVolume";
const KEY_VOLUME: &str = "objectSoundAmbientVolume";
const KEY_MIN_RANGE: &str = "objectSoundAmbientMinRange";
const KEY_MAX_RANGE: &str = "objectSoundAmbientMaxRange";
const KEY_PRIORITY: &str = "objectSoundAmbientPriority";

pub(crate) fn placed(
    properties: &Dictionary,
    template_ambient: Option<&str>,
    facts: impl Fn(&str, &Overrides) -> Option<EventFacts>,
) -> Option<Placed> {
    let none = Overrides::default();
    let named = match properties.get(KEY_AMBIENT) {
        Some(Value::Str(name) | Value::WideStr(name)) if name.is_empty() => return None,
        Some(Value::Str(name) | Value::WideStr(name)) => facts(name, &none).map(|_| name.clone()),
        _ => None,
    };
    let event = named.or_else(|| template_ambient.map(str::to_owned))?;
    let customized = matches!(properties.get(KEY_CUSTOMIZED), Some(Value::Bool(true)));
    let overrides = if customized {
        overrides_of(properties, &event, &facts)
    } else {
        none
    };
    let enabled = bool_of(properties.get(KEY_ENABLED))
        .unwrap_or_else(|| facts(&event, &overrides).is_none_or(|f| f.permanent));
    Some(Placed {
        event,
        overrides,
        enabled,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Custom {
    pub(crate) silenced: bool,
    pub(crate) sound: Option<(String, Overrides)>,
    pub(crate) enabled: bool,
}

pub(crate) fn customised(
    properties: &Dictionary,
    template_ambient: Option<&str>,
    facts: impl Fn(&str, &Overrides) -> Option<EventFacts>,
) -> Option<Custom> {
    let keys = [
        KEY_AMBIENT,
        KEY_CUSTOMIZED,
        KEY_ENABLED,
        KEY_LOOPING,
        KEY_MIN_VOLUME,
        KEY_VOLUME,
        KEY_MIN_RANGE,
        KEY_MAX_RANGE,
        KEY_PRIORITY,
    ];
    if !keys.iter().any(|key| properties.get(key).is_some()) {
        return None;
    }
    let none = Overrides::default();
    let named = match properties.get(KEY_AMBIENT) {
        Some(Value::Str(name) | Value::WideStr(name)) if name.is_empty() => {
            return Some(Custom {
                silenced: true,
                sound: None,
                enabled: false,
            });
        }
        Some(Value::Str(name) | Value::WideStr(name)) => facts(name, &none).map(|_| name.clone()),
        _ => None,
    };
    let customized = matches!(properties.get(KEY_CUSTOMIZED), Some(Value::Bool(true)));
    let sound = match named {
        Some(event) if customized => {
            let overrides = overrides_of(properties, &event, &facts);
            Some((event, overrides))
        }
        Some(event) => Some((event, none)),
        None if customized => template_ambient.map(|event| {
            let overrides = overrides_of(properties, event, &facts);
            (event.to_owned(), overrides)
        }),
        None => None,
    };
    let enabled = bool_of(properties.get(KEY_ENABLED)).unwrap_or_else(|| {
        let (event, overrides) = match &sound {
            Some((event, overrides)) => (Some(event.as_str()), *overrides),
            None => (template_ambient, none),
        };
        event.is_none_or(|event| facts(event, &overrides).is_none_or(|f| f.permanent))
    });
    Some(Custom {
        silenced: false,
        sound,
        enabled,
    })
}

fn overrides_of(
    properties: &Dictionary,
    event: &str,
    facts: &impl Fn(&str, &Overrides) -> Option<EventFacts>,
) -> Overrides {
    if facts(event, &Overrides::default()).is_none_or(|f| f.stream) {
        return Overrides::default();
    }
    Overrides {
        looping: bool_of(properties.get(KEY_LOOPING)),
        min_volume: real_of(properties.get(KEY_MIN_VOLUME)),
        volume: real_of(properties.get(KEY_VOLUME)),
        min_range: real_of(properties.get(KEY_MIN_RANGE)),
        max_range: real_of(properties.get(KEY_MAX_RANGE)),
        priority: match properties.get(KEY_PRIORITY) {
            Some(Value::Int(rank)) => priority_of(*rank),
            _ => None,
        },
    }
}

pub(crate) fn choose(
    custom: Option<&Custom>,
    rung: Rung,
    template: impl FnOnce() -> Option<String>,
) -> Option<(String, Overrides)> {
    match custom {
        Some(custom) if custom.silenced || !custom.enabled => None,
        Some(Custom {
            sound: Some(sound), ..
        }) if rung != Rung::Rubble => Some(sound.clone()),
        _ => template().map(|event| (event, Overrides::default())),
    }
}

fn bool_of(value: Option<&Value>) -> Option<bool> {
    match value {
        Some(Value::Bool(flag)) => Some(*flag),
        _ => None,
    }
}

fn real_of(value: Option<&Value>) -> Option<f32> {
    match value {
        Some(Value::Float(real)) => Some(*real),
        #[allow(clippy::cast_precision_loss)]
        Some(Value::Int(whole)) => Some(*whole as f32),
        _ => None,
    }
}

fn priority_of(rank: i32) -> Option<AudioPriority> {
    match rank {
        0 => Some(AudioPriority::Lowest),
        1 => Some(AudioPriority::Low),
        2 => Some(AudioPriority::Normal),
        3 => Some(AudioPriority::High),
        4 => Some(AudioPriority::Critical),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Waiting,
    Playing,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Play,
    Stop,
}

pub(crate) fn step(phase: Phase, looping: bool, reach: Reach, ready: bool) -> Action {
    match (phase, looping, reach) {
        (Phase::Done, ..) | (_, _, Reach::Unknown) => Action::None,
        (Phase::Waiting, false, _) if ready => Action::Play,
        (Phase::Waiting, true, Reach::Audible) if ready => Action::Play,
        (Phase::Playing, true, Reach::OutOfRange) => Action::Stop,
        _ => Action::None,
    }
}

pub(crate) fn after_play(looping: bool, outcome: Outcome, has_delay: bool) -> Phase {
    match outcome {
        Outcome::Queued if looping && has_delay => Phase::Waiting,
        Outcome::Queued if looping => Phase::Playing,
        Outcome::Shrouded if looping => Phase::Waiting,
        _ => Phase::Done,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Emitter {
    pub(crate) event: String,
    pub(crate) overrides: Overrides,
    pub(crate) at: [f32; 3],
    pub(crate) visible: bool,
    pub(crate) phase: Phase,
    pub(crate) looping: bool,
    pub(crate) cooldown: u32,
}

#[derive(Debug, Default)]
pub(crate) struct Emitters {
    pub(crate) map: BTreeMap<u64, Emitter>,
}

pub(crate) const PLACED: u64 = 1 << 63;

pub(crate) const BATTLE: u64 = 1 << 62;

pub(crate) const MOVING: u64 = 1 << 61;

pub(crate) const ABILITY_LOOP: u64 = 1 << 60;

impl Emitters {
    pub(crate) fn set(
        &mut self,
        owner: u64,
        wanted: Option<(String, Overrides)>,
        at: [f32; 3],
        visible: bool,
        looping: impl FnOnce(&str, &Overrides) -> bool,
    ) -> Option<String> {
        match (self.map.get_mut(&owner), wanted) {
            (Some(emitter), Some((event, overrides))) if emitter.event == event => {
                emitter.at = at;
                emitter.visible = visible;
                emitter.overrides = overrides;
                None
            }
            (_, Some((event, overrides))) => {
                let looping = looping(&event, &overrides);
                self.map
                    .insert(
                        owner,
                        Emitter {
                            event,
                            overrides,
                            at,
                            visible,
                            phase: Phase::Waiting,
                            looping,
                            cooldown: 0,
                        },
                    )
                    .filter(|old| old.phase == Phase::Playing)
                    .map(|old| old.event)
            }
            (Some(_), None) => self
                .map
                .remove(&owner)
                .filter(|old| old.phase == Phase::Playing)
                .map(|old| old.event),
            (None, None) => None,
        }
    }

    pub(crate) fn retain_units(&mut self, seen: &[u64]) -> Vec<(u64, String)> {
        let gone: Vec<u64> = self
            .map
            .keys()
            .copied()
            .filter(|owner| owner & PLACED == 0 && seen.binary_search(owner).is_err())
            .collect();
        gone.into_iter()
            .filter_map(|owner| {
                self.map
                    .remove(&owner)
                    .filter(|old| old.phase == Phase::Playing)
                    .map(|old| (owner, old.event))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
