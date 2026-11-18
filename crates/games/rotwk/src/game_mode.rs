#![allow(dead_code)]
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum GameMode {
    SinglePlayer = 0,
    Lan = 1,
    #[default]
    Skirmish = 2,
    Replay = 3,
    Shell = 4,
    Internet = 5,
    SinglePlayerPractice = 6,
    CreateAHero = 7,
    None = 9,
}

impl GameMode {
    pub(crate) fn from_number(number: u8) -> Option<Self> {
        Some(match number {
            0 => Self::SinglePlayer,
            1 => Self::Lan,
            2 => Self::Skirmish,
            3 => Self::Replay,
            4 => Self::Shell,
            5 => Self::Internet,
            6 => Self::SinglePlayerPractice,
            7 => Self::CreateAHero,
            9 => Self::None,
            _ => return Option::None,
        })
    }

    fn effective(self, recorded: Option<Self>) -> Self {
        match (self, recorded) {
            (Self::Replay, Some(recorded)) => recorded,
            _ => self,
        }
    }

    pub(crate) fn is_single_player(self, recorded: Option<Self>) -> bool {
        matches!(
            self.effective(recorded),
            Self::SinglePlayer | Self::SinglePlayerPractice
        )
    }

    pub(crate) fn is_multiplayer(self) -> bool {
        matches!(self, Self::Lan | Self::Internet)
    }

    pub(crate) fn is_multiplayer_or_replay_of(self, recorded: Option<Self>) -> bool {
        self.effective(recorded).is_multiplayer()
    }

    pub(crate) fn is_skirmish_or_multiplayer(self, recorded: Option<Self>) -> bool {
        let mode = self.effective(recorded);
        mode.is_multiplayer() || mode == Self::Skirmish
    }

    pub(crate) fn is_in_game(self) -> bool {
        !matches!(self, Self::None | Self::Shell | Self::CreateAHero)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeLineMode {
    Campaign = 1,
    Skirmish = 2,
    Lan = 3,
    Internet = 4,
    Replay = 5,
    WotrSingle = 6,
    WotrOnlineLan = 7,
    WotrCampaign = 8,
}

impl TimeLineMode {
    pub(crate) fn for_match(
        mode: GameMode,
        replay_playback: bool,
        recorded: Option<GameMode>,
    ) -> Option<Self> {
        if replay_playback {
            return Some(Self::Replay);
        }
        if mode.is_single_player(recorded) {
            return Some(Self::Campaign);
        }
        match mode {
            GameMode::Internet => Some(Self::Internet),
            GameMode::Lan => Some(Self::Lan),
            GameMode::Skirmish => Some(Self::Skirmish),
            _ => Option::None,
        }
    }

    pub(crate) fn screen_mode(self) -> &'static str {
        match self {
            Self::Campaign | Self::Skirmish | Self::WotrCampaign => "OtherSingle",
            Self::Lan | Self::Internet | Self::Replay => "OtherOnlineLan",
            Self::WotrSingle => "WOTRSingle",
            Self::WotrOnlineLan => "WOTROnlineLan",
        }
    }
}

impl fmt::Display for TimeLineMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.screen_mode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_mode_string_follows_the_timeline_mode() {
        let table = [
            (TimeLineMode::Campaign, "OtherSingle"),
            (TimeLineMode::Skirmish, "OtherSingle"),
            (TimeLineMode::Lan, "OtherOnlineLan"),
            (TimeLineMode::Internet, "OtherOnlineLan"),
            (TimeLineMode::Replay, "OtherOnlineLan"),
            (TimeLineMode::WotrSingle, "WOTRSingle"),
            (TimeLineMode::WotrOnlineLan, "WOTROnlineLan"),
            (TimeLineMode::WotrCampaign, "OtherSingle"),
        ];
        for (mode, text) in table {
            assert_eq!(mode.screen_mode(), text, "{mode:?}");
        }
    }

    #[test]
    fn the_opener_maps_each_game_mode_to_its_timeline_mode() {
        let none = None;
        let table = [
            (GameMode::SinglePlayer, TimeLineMode::Campaign),
            (GameMode::SinglePlayerPractice, TimeLineMode::Campaign),
            (GameMode::Skirmish, TimeLineMode::Skirmish),
            (GameMode::Lan, TimeLineMode::Lan),
            (GameMode::Internet, TimeLineMode::Internet),
        ];
        for (mode, expected) in table {
            assert_eq!(TimeLineMode::for_match(mode, false, none), Some(expected));
        }
        for mode in [GameMode::Shell, GameMode::CreateAHero, GameMode::None] {
            assert_eq!(TimeLineMode::for_match(mode, false, none), None);
        }
        assert_eq!(
            TimeLineMode::for_match(GameMode::Replay, true, Some(GameMode::Skirmish)),
            Some(TimeLineMode::Replay)
        );
    }

    #[test]
    fn predicates_look_through_a_replay_to_its_recorded_mode() {
        let replay = GameMode::Replay;
        assert!(replay.is_single_player(Some(GameMode::SinglePlayer)));
        assert!(!replay.is_single_player(None));
        assert!(replay.is_skirmish_or_multiplayer(Some(GameMode::Skirmish)));
        assert!(replay.is_skirmish_or_multiplayer(Some(GameMode::Internet)));
        assert!(!replay.is_skirmish_or_multiplayer(Some(GameMode::SinglePlayer)));
        assert!(!GameMode::Skirmish.is_multiplayer());
        assert!(GameMode::Skirmish.is_skirmish_or_multiplayer(None));
        assert!(!GameMode::Skirmish.is_multiplayer_or_replay_of(None));
        assert!(GameMode::SinglePlayerPractice.is_single_player(None));
        assert!(!GameMode::CreateAHero.is_in_game());
        assert!(GameMode::Skirmish.is_in_game());
    }

    #[test]
    fn mode_numbers_are_the_retail_ones() {
        for mode in [
            GameMode::SinglePlayer,
            GameMode::Lan,
            GameMode::Skirmish,
            GameMode::Replay,
            GameMode::Shell,
            GameMode::Internet,
            GameMode::SinglePlayerPractice,
            GameMode::CreateAHero,
            GameMode::None,
        ] {
            assert_eq!(GameMode::from_number(mode as u8), Some(mode));
        }
        assert_eq!(GameMode::from_number(8), None);
    }
}
