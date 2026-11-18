use mint_sage_ini::{LinearCampaignDef, LinearCampaignIndex};

use crate::lobby::Difficulty;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MissionSelector {
    Position(usize),
    Name(String),
}

impl MissionSelector {
    pub(crate) fn parse(text: &str) -> Self {
        match text.parse::<usize>() {
            Ok(position) => Self::Position(position),
            Err(_) => Self::Name(text.to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CampaignDev {
    pub(crate) campaign: String,
    pub(crate) mission: Option<MissionSelector>,
    pub(crate) difficulty: Difficulty,
}

impl CampaignDev {
    pub(crate) fn from_flags(
        campaign: Option<String>,
        mission: Option<String>,
        difficulty: Option<String>,
    ) -> Result<Option<Self>, String> {
        let Some(campaign) = campaign else {
            if mission.is_some() || difficulty.is_some() {
                return Err("--mission and --difficulty need --campaign".to_owned());
            }
            return Ok(None);
        };
        Ok(Some(Self {
            campaign,
            mission: mission.as_deref().map(MissionSelector::parse),
            difficulty: match difficulty {
                Some(text) => parse_difficulty(&text)?,
                None => Difficulty::Medium,
            },
        }))
    }

    pub(crate) fn validate(&self, vfs: &mint_vfs::Vfs) -> Result<(), String> {
        let index = crate::campaign_flow::linear_campaigns(vfs);
        let def = resolve_campaign(&index, &self.campaign)?;
        resolve_mission(def, self.mission.as_ref()).map(|_| ())
    }
}

pub(crate) fn parse_tutorial(
    flag: Option<&str>,
    campaign_given: bool,
) -> Result<Option<crate::tutorial::TutorialKind>, String> {
    use crate::tutorial::TutorialKind;
    let Some(text) = flag else {
        return Ok(None);
    };
    if campaign_given {
        return Err("--tutorial and --campaign start different things; give one".to_owned());
    }
    match text.to_ascii_lowercase().as_str() {
        "basic" => Ok(Some(TutorialKind::Basic)),
        "advanced" => Ok(Some(TutorialKind::Advanced)),
        _ => Err(format!("--tutorial wants basic or advanced, not {text:?}")),
    }
}

pub(crate) fn parse_difficulty(text: &str) -> Result<Difficulty, String> {
    match text.to_ascii_lowercase().as_str() {
        "easy" => Ok(Difficulty::Easy),
        "normal" | "medium" => Ok(Difficulty::Medium),
        "hard" => Ok(Difficulty::Hard),
        _ => Err(format!(
            "--difficulty wants easy, normal or hard, not {text:?}"
        )),
    }
}

pub(crate) fn difficulty_label(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Easy => "easy",
        Difficulty::Hard | Difficulty::Brutal => "hard",
        Difficulty::Medium => "normal",
    }
}

pub(crate) fn resolve_campaign<'a>(
    index: &'a LinearCampaignIndex,
    name: &str,
) -> Result<&'a LinearCampaignDef, String> {
    index.get(name).ok_or_else(|| {
        let names: Vec<&str> = index.iter().map(|def| def.name.as_str()).collect();
        format!(
            "no LinearCampaign named {name:?}; valid names: {}",
            names.join(", ")
        )
    })
}

pub(crate) fn resolve_mission(
    def: &LinearCampaignDef,
    selector: Option<&MissionSelector>,
) -> Result<u32, String> {
    let found = match selector {
        None => (!def.missions.is_empty()).then_some(0),
        Some(MissionSelector::Position(position)) => position
            .checked_sub(1)
            .filter(|at| *at < def.missions.len()),
        Some(MissionSelector::Name(name)) => def
            .missions
            .iter()
            .position(|mission| mission.name.eq_ignore_ascii_case(name)),
    };
    found.and_then(|at| u32::try_from(at).ok()).ok_or_else(|| {
        let listed: Vec<String> = def
            .missions
            .iter()
            .enumerate()
            .map(|(at, mission)| format!("{} {}", at.saturating_add(1), mission.name))
            .collect();
        format!(
            "campaign {} has no such mission; valid missions: {}",
            def.name,
            listed.join(", ")
        )
    })
}

#[cfg(test)]
mod tests;
