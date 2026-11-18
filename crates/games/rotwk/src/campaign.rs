use mint_sage_campaign::{CampaignState, Carryover, SeedUnit};
use mint_sage_ini::{LinearCampaignDef, LinearCampaignIndex, MissionDef};

use crate::lobby::Difficulty;
use crate::match_start::MatchStartPresentation;

pub(crate) fn difficulty_from_level(level: &str) -> Difficulty {
    match level.chars().next() {
        Some('E') => Difficulty::Easy,
        Some('H') => Difficulty::Hard,
        _ => Difficulty::Medium,
    }
}

pub(crate) fn difficulty_number(difficulty: Difficulty) -> u8 {
    match difficulty {
        Difficulty::Easy => 0,
        Difficulty::Hard | Difficulty::Brutal => 2,
        Difficulty::Medium => 1,
    }
}

fn map_path(name: &str) -> String {
    format!("maps/{name}/{name}.map")
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CampaignMatch {
    pub(crate) map: String,
    pub(crate) difficulty: Difficulty,
    pub(crate) carryover: Carryover,
    pub(crate) delayed: Vec<String>,
    pub(crate) mode: crate::game_mode::GameMode,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SideFacts {
    pub(crate) neutral: bool,
    pub(crate) human: bool,
}

pub(crate) fn pick_local(sides: &[SideFacts]) -> Option<usize> {
    sides
        .iter()
        .position(|s| !s.neutral && s.human)
        .or_else(|| sides.iter().position(|s| !s.neutral))
}

pub(crate) fn local_side(sides: &[mint_sage_map::Side]) -> Option<usize> {
    let facts: Vec<SideFacts> = sides
        .iter()
        .map(|side| SideFacts {
            neutral: side.name().is_none_or(str::is_empty),
            human: matches!(
                side.properties.get("playerIsHuman"),
                Some(mint_sage_map::Value::Bool(true))
            ),
        })
        .collect();
    pick_local(&facts)
}

#[derive(Debug, Clone)]
pub(crate) struct CampaignRun {
    pub(crate) def: LinearCampaignDef,
    pub(crate) state: CampaignState,
    pub(crate) carry: Carryover,
    pub(crate) unlocks_bonus: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct MissionPlan {
    pub(crate) config: CampaignMatch,
    pub(crate) presentation: MatchStartPresentation,
}

impl CampaignRun {
    pub(crate) fn new(
        index: &LinearCampaignIndex,
        key: &str,
        difficulty: Difficulty,
        unlocks_bonus: bool,
    ) -> Option<Self> {
        let def = index.get(key)?.clone();
        let count = u32::try_from(def.missions.len()).unwrap_or(u32::MAX);
        let carry = Carryover::seed(
            def.carryover_units
                .iter()
                .map(|name| SeedUnit { name, hero: false }),
        );
        Some(Self {
            state: CampaignState::new(count, difficulty_number(difficulty)),
            carry,
            def,
            unlocks_bonus,
        })
    }

    pub(crate) fn from_save(
        index: &LinearCampaignIndex,
        key: &str,
        difficulty: u32,
        unlocks_bonus: bool,
        saved: &mint_sage_save::CampaignState,
    ) -> Option<Self> {
        let def = index.get(key)?.clone();
        let count = u32::try_from(def.missions.len()).unwrap_or(u32::MAX);
        let level = u8::try_from(difficulty).unwrap_or(1);
        Some(Self {
            state: CampaignState::from_save(count, level, saved),
            carry: CampaignState::carryover_from_save(saved),
            def,
            unlocks_bonus,
        })
    }

    pub(crate) fn difficulty(&self) -> Difficulty {
        match self.state.difficulty() {
            0 => Difficulty::Easy,
            2 => Difficulty::Hard,
            _ => Difficulty::Medium,
        }
    }

    pub(crate) fn mission(&self, index: u32) -> Option<&MissionDef> {
        self.def.missions.get(usize::try_from(index).ok()?)
    }

    pub(crate) fn plan(&self, index: u32, play_intro_movie: bool) -> Option<MissionPlan> {
        let mission = self.mission(index)?;
        Some(MissionPlan {
            config: CampaignMatch {
                map: map_path(mission.map.as_deref()?),
                difficulty: self.difficulty(),
                carryover: self.carry.clone(),
                delayed: mission.delay_carryover_spawning_of.clone(),
                mode: crate::game_mode::GameMode::SinglePlayer,
            },
            presentation: MatchStartPresentation::for_mission(mission, play_intro_movie),
        })
    }
}

#[cfg(test)]
mod tests;
