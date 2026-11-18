use mint_sage_ini::living_world::campaign::{LivingWorldCampaignDef, TutorialTask};
use mint_sage_ini::living_world::LivingWorldData;

use crate::model::{Phase, PlayerKind, RegionId};
use crate::setup::{PlayerSetup, Setup};
use crate::state::LivingWorldState;

pub const FRAMES_PER_SECOND: f32 = 5.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task {
    Unhandled(TutorialTask),
    BuildBarracks(RegionId),
    TrainUnit(RegionId),
    MergeGarrison { region: RegionId, army: String },
    MoveHeroArmyToRegion { army: String, region: RegionId },
    OpenDetailsPanel(RegionId),
    SelectBattle(RegionId),
    EnableAutoResolve(RegionId),
    RetreatArmies,
    ScriptedArmyMove { army: String, region: RegionId },
    ScriptedDelay { frames: u32 },
    EndTutorial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TaskScript {
    pub task: Task,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionScript {
    pub audio: Option<String>,
    pub text: Option<String>,
    pub clear_previous_text: bool,
    pub task: Option<TaskScript>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Block {
    pub turn: u32,
    pub phase: Phase,
    pub sessions: Vec<SessionScript>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TutorialError {
    UnknownCampaign(String),
    NoTutorial(String),
    BadTurn(String),
    BadPhase(String),
    BadParams {
        task: String,
        wanted: usize,
        got: usize,
    },
    BadDelay(String),
    UnknownRegion(String),
    UnknownArmy(String),
    NoLocalPlayer,
}

impl std::fmt::Display for TutorialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TutorialError {}

fn phase_named(name: &str) -> Option<Phase> {
    [
        ("Planning", Phase::Planning),
        ("MoveArmies", Phase::MoveArmies),
        ("ResolveBattles", Phase::ResolveBattles),
        ("FakePhaseOne", Phase::FakePhaseOne),
        ("PlanRetreats", Phase::PlanRetreats),
        ("RetreatArmies", Phase::RetreatArmies),
        ("Complete", Phase::Complete),
    ]
    .into_iter()
    .find(|(n, _)| n.eq_ignore_ascii_case(name))
    .map(|(_, p)| p)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn delay_frames(text: &str) -> Option<u32> {
    let seconds: f32 = text.trim().parse().ok()?;
    (seconds.is_finite() && seconds >= 0.0)
        .then(|| (seconds * FRAMES_PER_SECOND).trunc().min(u32::MAX as f32) as u32)
}

struct Reader<'a> {
    state: &'a LivingWorldState,
    campaign: &'a LivingWorldCampaignDef,
}

impl Reader<'_> {
    fn region(&self, name: &str) -> Result<RegionId, TutorialError> {
        self.state
            .rules
            .region(name)
            .map_err(|_| TutorialError::UnknownRegion(name.to_owned()))
    }

    fn army(&self, name: &str) -> Result<String, TutorialError> {
        self.campaign
            .scripting_names()
            .any(|n| n.eq_ignore_ascii_case(name))
            .then(|| name.to_owned())
            .ok_or_else(|| TutorialError::UnknownArmy(name.to_owned()))
    }

    fn task(&self, task: TutorialTask, params: &[String]) -> Result<Option<Task>, TutorialError> {
        let wanted = match task {
            TutorialTask::Invalid => return Ok(None),
            TutorialTask::EndPhase | TutorialTask::ZoomScroll => {
                return Ok(Some(Task::Unhandled(task)))
            }
            TutorialTask::RetreatArmies | TutorialTask::EndTutorial => 0,
            TutorialTask::MergeGarrison
            | TutorialTask::MoveHeroArmyToRegion
            | TutorialTask::ScriptedArmyMove => 2,
            _ => 1,
        };
        if params.len() != wanted {
            return Err(TutorialError::BadParams {
                task: format!("{task:?}"),
                wanted,
                got: params.len(),
            });
        }
        let at = |i: usize| params.get(i).map_or("", String::as_str);
        Ok(Some(match task {
            TutorialTask::BuildBarracks => Task::BuildBarracks(self.region(at(0))?),
            TutorialTask::TrainUnit => Task::TrainUnit(self.region(at(0))?),
            TutorialTask::MergeGarrison => Task::MergeGarrison {
                region: self.region(at(0))?,
                army: self.army(at(1))?,
            },
            TutorialTask::MoveHeroArmyToRegion => Task::MoveHeroArmyToRegion {
                army: self.army(at(0))?,
                region: self.region(at(1))?,
            },
            TutorialTask::OpenDetailsPanel => Task::OpenDetailsPanel(self.region(at(0))?),
            TutorialTask::SelectBattle => Task::SelectBattle(self.region(at(0))?),
            TutorialTask::EnableAutoResolveButtonAndWaitForCompletion => {
                Task::EnableAutoResolve(self.region(at(0))?)
            }
            TutorialTask::RetreatArmies => Task::RetreatArmies,
            TutorialTask::ScriptedArmyMove => Task::ScriptedArmyMove {
                army: self.army(at(0))?,
                region: self.region(at(1))?,
            },
            TutorialTask::ScriptedDelay => Task::ScriptedDelay {
                frames: delay_frames(at(0))
                    .ok_or_else(|| TutorialError::BadDelay(at(0).to_owned()))?,
            },
            TutorialTask::EndTutorial => Task::EndTutorial,
            TutorialTask::EndPhase | TutorialTask::ZoomScroll | TutorialTask::Invalid => {
                return Ok(None)
            }
        }))
    }
}

pub(super) fn read(
    data: &LivingWorldData,
    state: &LivingWorldState,
) -> Result<Vec<Block>, TutorialError> {
    let name = &state.rules.campaign;
    let campaign = data
        .campaigns
        .get(name)
        .ok_or_else(|| TutorialError::UnknownCampaign(name.clone()))?;
    let tutorial = campaign
        .tutorials
        .first()
        .ok_or_else(|| TutorialError::NoTutorial(name.clone()))?;
    let reader = Reader { state, campaign };
    let mut blocks = Vec::new();
    for turn in &tutorial.turns {
        let number: u32 = turn
            .name
            .trim()
            .parse()
            .map_err(|_| TutorialError::BadTurn(turn.name.clone()))?;
        for phase in &turn.phases {
            let phase_id = phase_named(phase.name.trim())
                .ok_or_else(|| TutorialError::BadPhase(phase.name.clone()))?;
            let mut sessions = Vec::new();
            for session in &phase.sessions {
                let task = match session.tasks.last() {
                    Some(def) => match def.task {
                        Some(kind) => reader.task(kind, &def.params.0)?.map(|task| TaskScript {
                            task,
                            text: def.text.clone(),
                        }),
                        None => None,
                    },
                    None => None,
                };
                sessions.push(SessionScript {
                    audio: session.audio.clone(),
                    text: session.text.clone(),
                    clear_previous_text: session.clear_previous_text == Some(true),
                    task,
                });
            }
            blocks.push(Block {
                turn: number,
                phase: phase_id,
                sessions,
            });
        }
    }
    Ok(blocks)
}

pub fn scripted_setup(
    data: &LivingWorldData,
    campaign: &str,
    seed: u64,
) -> Result<Setup, TutorialError> {
    let def = data
        .campaigns
        .get(campaign)
        .ok_or_else(|| TutorialError::UnknownCampaign(campaign.to_owned()))?;
    let local = def.local_player.as_deref().unwrap_or_default();
    if !def
        .add_players
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case(local))
    {
        return Err(TutorialError::NoLocalPlayer);
    }
    let players = def
        .add_players
        .iter()
        .map(|p| PlayerSetup {
            template: p.player_template.clone().unwrap_or_default(),
            kind: if p.is_dumb == Some(true) {
                PlayerKind::Inert
            } else if p.name.eq_ignore_ascii_case(local) {
                PlayerKind::Human
            } else {
                PlayerKind::Computer
            },
            team: p.team_number.unwrap_or(-1),
            start_region: p.base_region.clone(),
            skill: crate::ai::Skill::Medium,
        })
        .collect();
    Ok(Setup {
        campaign: campaign.to_owned(),
        scenario: 0,
        victory_type: 0,
        battle_type: crate::screen_queries::BattleType::default(),
        seed,
        players,
    })
}
