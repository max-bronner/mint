#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod carryover;
mod placement;

pub use carryover::{CapturedObject, Carryover, PlayerCapture, ScienceFact, SeedUnit, UpgradeFact};
pub use placement::{walk_on_anchor, WalkOnAnchor, RALLY_FALLBACK_X};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    StartMission {
        index: u32,
        play_intro_movie: bool,
        restore_carryover_snapshot: bool,
    },
    MainMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeLineClose {
    CampaignComplete,
    CampaignMenu { autosave: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionEnd {
    Victory,
    Defeat,
}

impl MissionEnd {
    #[must_use]
    pub fn from_action(name: &str) -> Option<Self> {
        let is = |wanted: &str| name.eq_ignore_ascii_case(wanted);
        if is("VICTORY") || is("QUICKVICTORY") {
            Some(Self::Victory)
        } else if is("DEFEAT") || is("LOCALDEFEAT") {
            Some(Self::Defeat)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CampaignState {
    mission_count: u32,
    difficulty: u8,
    index: u32,
    victorious: bool,
    defeated: bool,
    ended: bool,
    advanced: bool,
}

impl CampaignState {
    #[must_use]
    pub fn new(mission_count: u32, difficulty: u8) -> Self {
        Self {
            mission_count,
            difficulty,
            index: 0,
            victorious: false,
            defeated: false,
            ended: false,
            advanced: false,
        }
    }

    #[must_use]
    pub fn from_save(
        mission_count: u32,
        difficulty: u8,
        saved: &mint_sage_save::CampaignState,
    ) -> Self {
        Self {
            mission_count,
            difficulty,
            index: u32::try_from(saved.mission_index).unwrap_or(0),
            victorious: saved.mission_won,
            defeated: saved.mission_lost,
            ended: saved.mission_ended,
            advanced: saved.index_advanced,
        }
    }

    #[must_use]
    pub fn to_save(&self, carry: &Carryover) -> mint_sage_save::CampaignState {
        let mut saved = mint_sage_save::CampaignState {
            mission_index: i32::try_from(self.index).unwrap_or(i32::MAX),
            index_advanced: self.advanced,
            mission_won: self.victorious,
            mission_lost: self.defeated,
            mission_ended: self.ended,
            ..mint_sage_save::CampaignState::default()
        };
        carry.fill(&mut saved);
        saved
    }

    #[must_use]
    pub fn carryover_from_save(saved: &mint_sage_save::CampaignState) -> Carryover {
        Carryover::from_state(saved)
    }

    pub fn load(&mut self) -> Outcome {
        if self.ended {
            self.advance_once();
        }
        self.start_mission(true, false)
    }

    #[must_use]
    pub fn difficulty(&self) -> u8 {
        self.difficulty
    }

    #[must_use]
    pub fn index(&self) -> u32 {
        self.index
    }

    #[must_use]
    pub fn victorious(&self) -> bool {
        self.victorious
    }

    #[must_use]
    pub fn defeated(&self) -> bool {
        self.defeated
    }

    #[must_use]
    pub fn ended(&self) -> bool {
        self.ended
    }

    #[must_use]
    pub fn advanced(&self) -> bool {
        self.advanced
    }

    pub fn start(&mut self) -> Outcome {
        self.start_at(0)
    }

    pub fn start_at(&mut self, index: u32) -> Outcome {
        self.index = index;
        self.advanced = false;
        self.start_mission(true, false)
    }

    pub fn script_action(&mut self, name: &str) -> bool {
        match MissionEnd::from_action(name) {
            Some(MissionEnd::Victory) => self.victorious = true,
            Some(MissionEnd::Defeat) => self.defeated = true,
            None => return false,
        }
        true
    }

    pub fn match_torn_down(&mut self) {
        self.ended = true;
        if self.victorious {
            self.advance_once();
        }
    }

    #[must_use]
    pub fn has_next_mission(&self) -> bool {
        let next = if self.advanced {
            self.index
        } else {
            self.index.saturating_add(1)
        };
        next < self.mission_count
    }

    #[must_use]
    pub fn menu_mission_index(&self) -> u32 {
        if self.victorious && !self.advanced {
            self.index.saturating_add(1)
        } else {
            self.index
        }
    }

    #[must_use]
    pub fn close_time_line(&self) -> TimeLineClose {
        if self.victorious && !self.has_next_mission() {
            TimeLineClose::CampaignComplete
        } else {
            TimeLineClose::CampaignMenu {
                autosave: self.victorious,
            }
        }
    }

    pub fn next_mission(&mut self) -> Outcome {
        self.advance_once();
        self.start_mission(true, false)
    }

    pub fn last_mission(&mut self) -> Outcome {
        if self.victorious || self.advanced {
            return self.next_mission();
        }
        self.defeated = false;
        self.start_mission(false, true)
    }

    fn advance_once(&mut self) {
        if !self.advanced {
            self.index = self.index.saturating_add(1);
            self.advanced = true;
        }
    }

    fn start_mission(&mut self, play_intro_movie: bool, restore_snapshot: bool) -> Outcome {
        let missing = self.index >= self.mission_count;
        let end = missing || self.defeated;
        self.victorious = false;
        self.defeated = false;
        self.ended = false;
        self.advanced = false;
        if end {
            return Outcome::MainMenu;
        }
        Outcome::StartMission {
            index: self.index,
            play_intro_movie,
            restore_carryover_snapshot: restore_snapshot,
        }
    }
}

#[cfg(test)]
mod tests;
