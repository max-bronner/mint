use mint_sage_ini::{Block, ValueErrors};

use crate::campaign::{CampaignMatch, MissionPlan};
use crate::game_mode::GameMode;
use crate::lobby::Difficulty;
use crate::match_start::{MatchStartPresentation, DEFAULT_FADE_UP_FRAMES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TutorialKind {
    Basic,
    Advanced,
}

impl TutorialKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Advanced => "Advanced",
        }
    }

    fn prefix(self) -> &'static str {
        self.label()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnTutorial {
    Start(TutorialKind),
    WarOfTheRing,
    Nothing,
}

impl OnTutorial {
    pub(crate) fn from_arg(arg: &str) -> Self {
        if arg.eq_ignore_ascii_case("Basic") {
            Self::Start(TutorialKind::Basic)
        } else if arg.eq_ignore_ascii_case("Advanced") {
            Self::Start(TutorialKind::Advanced)
        } else if arg.eq_ignore_ascii_case("Strategic") {
            Self::WarOfTheRing
        } else {
            Self::Nothing
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TutorialBlock {
    pub(crate) map: Option<String>,
    pub(crate) load_movie: Option<String>,
    pub(crate) load_still: Option<String>,
    pub(crate) load_voice: Option<String>,
    pub(crate) load_music: Option<String>,
    pub(crate) objective: Vec<String>,
    pub(crate) fade_up_frames: u32,
}

impl Default for TutorialBlock {
    fn default() -> Self {
        Self {
            map: None,
            load_movie: None,
            load_still: None,
            load_voice: None,
            load_music: None,
            objective: Vec::new(),
            fade_up_frames: DEFAULT_FADE_UP_FRAMES,
        }
    }
}

impl TutorialBlock {
    fn read(block: &Block, kind: TutorialKind, errors: &ValueErrors) -> Self {
        let name = |suffix: &str| format!("{}Tutorial{suffix}", kind.prefix());
        let text = |suffix: &str| {
            block
                .field(&name(suffix))
                .and_then(mint_sage_ini::Field::value)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let fade_field = name("MillisecondsAfterStartToStartFadeUp");
        let fade_up_frames = block
            .field(&fade_field)
            .map_or(DEFAULT_FADE_UP_FRAMES, |field| {
                let read = field
                    .value()
                    .and_then(mint_sage_ini::value::parse_int)
                    .and_then(|ms| u32::try_from(ms).ok());
                if read.is_none() {
                    errors.field("GameData", field, "a non-negative integer");
                }
                read.map_or(DEFAULT_FADE_UP_FRAMES, mint_sage_ini::ms_to_frames)
            });
        Self {
            map: text("Map"),
            load_movie: text("LoadMovie"),
            load_still: text("LoadScreenStillImage"),
            load_voice: text("LoadScreenVoiceTrack"),
            load_music: text("LoadScreenMusicTrack"),
            objective: block
                .fields_named(&name("Objective"))
                .flat_map(|field| field.args.iter().cloned())
                .collect(),
            fade_up_frames,
        }
    }

    pub(crate) fn plan(&self) -> Option<MissionPlan> {
        let map = self.map.as_deref()?.replace('\\', "/");
        Some(MissionPlan {
            config: CampaignMatch {
                map,
                difficulty: Difficulty::Easy,
                carryover: mint_sage_campaign::Carryover::default(),
                delayed: Vec::new(),
                mode: GameMode::SinglePlayerPractice,
            },
            presentation: MatchStartPresentation {
                intro_movie: self.load_movie.clone(),
                load_still: self.load_still.clone(),
                load_voice: self.load_voice.clone(),
                load_music: self.load_music.clone(),
                fade_up_frames: self.fade_up_frames,
            },
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Tutorials {
    basic: TutorialBlock,
    advanced: TutorialBlock,
}

impl Tutorials {
    pub(crate) fn read(block: &Block, errors: &ValueErrors) -> Self {
        Self {
            basic: TutorialBlock::read(block, TutorialKind::Basic, errors),
            advanced: TutorialBlock::read(block, TutorialKind::Advanced, errors),
        }
    }

    pub(crate) fn block(&self, kind: TutorialKind) -> &TutorialBlock {
        match kind {
            TutorialKind::Basic => &self.basic,
            TutorialKind::Advanced => &self.advanced,
        }
    }
}

#[cfg(test)]
mod tests;
