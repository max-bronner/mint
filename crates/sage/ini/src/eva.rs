use crate::tree::{Block, Document, Field};
use crate::value::{is_none, parse_bool, parse_float, MacroTable};

pub const PREDEFINED_EVA_EVENT_KEYWORD: &str = "PredefinedEvaEvent";
pub const NEW_EVA_EVENT_KEYWORD: &str = "NewEvaEvent";
pub const SCORED_KILL_EVA_ANNOUNCER_KEYWORD: &str = "ScoredKillEvaAnnouncer";
pub const DEFAULT_EVA_EVENT: &str = "DefaultEvaEvent";
pub const MISC_EVA_DATA_KEYWORD: &str = "MiscEvaData";

pub const PREDEFINED_EVA_EVENT_NAMES: [&str; 22] = [
    "DefaultEvaEvent",
    "BeaconDetected",
    "GeneralLevelUp",
    "UnitLevelUp",
    "UpgradeComplete",
    "CastleBreached",
    "AllyDefeated",
    "EnemyDefeated",
    "EnemyCampDestroyed",
    "CampDestroyed",
    "AllyCampDestroyed",
    "BuildQueuePausedDueToCPLimit",
    "CannotBuildDueToCPLimit",
    "BuildQueuePausedDueToFunds",
    "CannotBuildDueToFunds",
    "WallsBeingClimbed",
    "BuildingBeingStolen",
    "BuildingStolen",
    "WorldMustBattle",
    "WorldMustRetreat",
    "WorldMustChooseOwner",
    "WorldRegionLostUncontested",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaEventDef {
    pub name: String,
    pub priority: u32,
    pub time_between_events_ms: u32,
    pub expiration_time_ms: u32,
    pub quiet_time_ms: u32,
    pub wait_before_playing_ms: u32,
    pub always_play_from_home_base: bool,
    pub count_as_jump_to_location: bool,
    pub side_sounds: Vec<(String, Option<String>)>,
    pub blocks: Vec<String>,
}

impl EvaEventDef {
    fn defaults(name: &str) -> Self {
        Self {
            name: name.to_string(),
            priority: 1,
            time_between_events_ms: 0,
            expiration_time_ms: 0,
            quiet_time_ms: 0,
            wait_before_playing_ms: 0,
            always_play_from_home_base: false,
            count_as_jump_to_location: true,
            side_sounds: Vec::new(),
            blocks: Vec::new(),
        }
    }

    #[must_use]
    pub fn sound_for(&self, side: &str) -> Option<&str> {
        self.side_sounds
            .iter()
            .find(|(own, _)| own.eq_ignore_ascii_case(side))
            .and_then(|(_, sound)| sound.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredKillEvaAnnouncerDef {
    pub name: String,
    pub event: String,
    pub only_kills_by_local_player: bool,
    pub only_kills_against_local_player: bool,
    pub minimum_count: u32,
    pub maximum_time_ms: u32,
    pub object_filter: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpSettings {
    pub min_distance_between_jump_to_events: f32,
    pub max_milliseconds_before_resetting_last_jump_to: u32,
}

impl Default for JumpSettings {
    fn default() -> Self {
        Self {
            min_distance_between_jump_to_events: 600.0,
            max_milliseconds_before_resetting_last_jump_to: 2000,
        }
    }
}

#[must_use]
pub fn read_jump_settings(documents: &[Document]) -> JumpSettings {
    let mut settings = JumpSettings::default();
    for document in documents {
        for block in document.roots() {
            if !block.keyword.eq_ignore_ascii_case(MISC_EVA_DATA_KEYWORD) {
                continue;
            }
            if let Some(value) = block
                .field("MinDistanceBetweenJumpToEvents")
                .and_then(Field::value)
                .and_then(parse_float)
            {
                settings.min_distance_between_jump_to_events = value;
            }
            if let Some(value) = block
                .field("MaxMillisecondsBeforeResettingLastJumpTo")
                .and_then(Field::value)
                .and_then(parse_float)
            {
                settings.max_milliseconds_before_resetting_last_jump_to = ms_u32(value);
            }
        }
    }
    settings
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaFieldError {
    pub file: std::sync::Arc<str>,
    pub line: u32,
    pub keyword: String,
    pub token: String,
}

impl std::fmt::Display for EvaFieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: {} does not parse: {:?}",
            self.file, self.line, self.keyword, self.token
        )
    }
}

impl EvaFieldError {
    fn new(field: &Field, token: &str) -> Self {
        Self {
            file: std::sync::Arc::clone(&field.file),
            line: field.line,
            keyword: field.keyword.clone(),
            token: token.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvaDefs {
    pub events: Vec<EvaEventDef>,
    pub announcers: Vec<ScoredKillEvaAnnouncerDef>,
    pub malformed: Vec<EvaFieldError>,
}

impl EvaDefs {
    #[must_use]
    pub fn read(documents: &[Document], macros: &MacroTable) -> Self {
        let default_block = documents
            .iter()
            .flat_map(|d| d.roots_named(PREDEFINED_EVA_EVENT_KEYWORD))
            .filter(|b| {
                b.name()
                    .is_some_and(|n| n.eq_ignore_ascii_case(DEFAULT_EVA_EVENT))
            })
            .last();
        let mut malformed = Vec::new();
        let mut base = EvaEventDef::defaults(DEFAULT_EVA_EVENT);
        if let Some(block) = default_block {
            apply(&mut base, block, macros, &mut malformed);
        }
        let mut defs = Self {
            events: PREDEFINED_EVA_EVENT_NAMES
                .iter()
                .map(|name| EvaEventDef {
                    name: (*name).to_string(),
                    ..base.clone()
                })
                .collect(),
            announcers: Vec::new(),
            malformed: Vec::new(),
        };
        for document in documents {
            for block in document.roots() {
                let Some(name) = block.name() else { continue };
                if block.keyword.eq(PREDEFINED_EVA_EVENT_KEYWORD)
                    || block.keyword.eq(NEW_EVA_EVENT_KEYWORD)
                {
                    let index = match defs.position(name) {
                        Some(index) => index,
                        None => {
                            defs.events.push(EvaEventDef {
                                name: name.to_string(),
                                ..base.clone()
                            });
                            defs.events.len().saturating_sub(1)
                        }
                    };
                    if let Some(def) = defs.events.get_mut(index) {
                        apply(def, block, macros, &mut malformed);
                    }
                } else if block.keyword.eq(SCORED_KILL_EVA_ANNOUNCER_KEYWORD) {
                    defs.announcers.push(announcer(name, block, macros));
                }
            }
        }
        defs.malformed = malformed;
        defs
    }

    #[must_use]
    pub fn position(&self, name: &str) -> Option<usize> {
        self.events
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name))
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn ms_u32(value: f32) -> u32 {
    value.max(0.0) as u32
}

fn number(token: Option<&str>, macros: &MacroTable) -> Option<u32> {
    let token = token?;
    let value = parse_float(token).or_else(|| macros.get(token))?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(value.clamp(0.0, u32::MAX as f32) as u32)
}

fn apply(
    def: &mut EvaEventDef,
    block: &Block,
    macros: &MacroTable,
    malformed: &mut Vec<EvaFieldError>,
) {
    for field in block.fields() {
        let value = field.value();
        let target = match field.keyword.as_str() {
            "Priority" => &mut def.priority,
            "TimeBetweenEventsMS" => &mut def.time_between_events_ms,
            "ExpirationTimeMS" => &mut def.expiration_time_ms,
            "QuietTimeMS" => &mut def.quiet_time_ms,
            "MillisecondsToWaitBeforePlaying" => &mut def.wait_before_playing_ms,
            "AlwaysPlayFromHomeBase" => {
                match value.map(parse_bool) {
                    Some(Some(flag)) => def.always_play_from_home_base = flag,
                    Some(None) => malformed.push(EvaFieldError::new(field, value.unwrap_or(""))),
                    None => {}
                }
                continue;
            }
            "CountAsJumpToLocation" => {
                match value.map(parse_bool) {
                    Some(Some(flag)) => def.count_as_jump_to_location = flag,
                    Some(None) => malformed.push(EvaFieldError::new(field, value.unwrap_or(""))),
                    None => {}
                }
                continue;
            }
            "OtherEvaEventsToBlock" => {
                def.blocks = field.args.iter().filter(|a| !is_none(a)).cloned().collect();
                continue;
            }
            _ => continue,
        };
        match value.map(|token| number(Some(token), macros)) {
            Some(Some(n)) => *target = n,
            Some(None) => malformed.push(EvaFieldError::new(field, value.unwrap_or(""))),
            None => {}
        }
    }
    for side_sound in block.blocks_named("SideSound") {
        let side = side_sound
            .fields_named("Side")
            .filter_map(|f| f.value())
            .last();
        let sound = side_sound
            .fields_named("Sound")
            .filter_map(|f| f.value())
            .last()
            .filter(|s| !is_none(s) && !s.eq_ignore_ascii_case("NoSound"));
        if let Some(side) = side {
            def.side_sounds
                .retain(|(own, _)| !own.eq_ignore_ascii_case(side));
            def.side_sounds
                .push((side.to_string(), sound.map(str::to_string)));
        }
    }
}

fn announcer(name: &str, block: &Block, macros: &MacroTable) -> ScoredKillEvaAnnouncerDef {
    let flag = |keyword: &str| {
        block
            .field(keyword)
            .and_then(|f| f.value())
            .and_then(parse_bool)
            .unwrap_or(true)
    };
    let count = |keyword: &str| number(block.field(keyword).and_then(|f| f.value()), macros);
    ScoredKillEvaAnnouncerDef {
        name: name.to_string(),
        event: block
            .field("EvaEvent")
            .and_then(|f| f.value())
            .unwrap_or_default()
            .to_string(),
        only_kills_by_local_player: flag("CountOnlyKillsByLocalPlayer"),
        only_kills_against_local_player: flag("CountOnlyKillsAgainstLocalPlayer"),
        minimum_count: count("MinimumCountForAnnouncement").unwrap_or(10),
        maximum_time_ms: count("MaximumTimeForAnnouncementMS").unwrap_or(1000),
        object_filter: block
            .field("ObjectFilter")
            .map(|f| {
                let mut args = f.args.clone();
                crate::value::expand_token_list_macros(&mut args, macros);
                args
            })
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests;
