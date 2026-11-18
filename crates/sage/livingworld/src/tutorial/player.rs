use mint_sage_ini::living_world::players::BuildingType;

use super::{Blocked, SessionAudio, Task, Tick, Tutorial, TutorialEvent, UiAction};
use crate::battle::{reborrow, AutoResolve};
use crate::model::{BuildingId, Command, Event, Phase, PlayerId};
use crate::state::LivingWorldState;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Command(Command),
    Ui(UiAction),
}

fn building_of(
    state: &LivingWorldState,
    player: PlayerId,
    kind: BuildingType,
) -> Option<BuildingId> {
    let template = state.player(player)?.template;
    (0..state.rules.buildings.len()).find_map(|i| {
        let id = BuildingId(u16::try_from(i).ok()?);
        let b = state.rules.building(id)?;
        (b.kind == Some(kind) && (b.available_to.is_empty() || b.available_to.contains(&template)))
            .then_some(id)
    })
}

fn retreat_option(state: &LivingWorldState, player: PlayerId) -> Option<Command> {
    let army = *state.retreat_pending(player).first()?;
    let from = state.armies.get(&army)?.region;
    let dest = state
        .rules
        .region_rules(from)?
        .neighbours
        .iter()
        .copied()
        .find(|d| state.check_order(player, army, *d, true).is_ok())?;
    Some(Command::ChooseRetreat { army, dest })
}

#[must_use]
pub fn next_action(tutorial: &Tutorial, state: &LivingWorldState) -> Option<Action> {
    let player = Tutorial::local(state)?;
    let command = |c: Command| Some(Action::Command(c));
    let Some(task) = tutorial.active_task(state) else {
        let ready = state.player(player).is_some_and(|p| p.ready);
        return (state.phase == Phase::Planning && tutorial.exhausted(state) && !ready)
            .then_some(Action::Command(Command::EndPhase));
    };
    match task {
        Task::BuildBarracks(region) => {
            let plot = state
                .region(*region)?
                .plots
                .iter()
                .position(Option::is_none)?;
            command(Command::Build {
                region: *region,
                plot: u8::try_from(plot).ok()?,
                building: building_of(state, player, BuildingType::Barracks)?,
            })
        }
        Task::TrainUnit(region) => {
            let plot = tutorial.active_run(state)?.plot?;
            command(Command::QueueArmy {
                region: *region,
                plot,
                entry: (0..=u8::MAX).find(|e| Tutorial::offered(state, *region, plot, *e))?,
            })
        }
        Task::MergeGarrison { army, .. } => {
            let named = state.scripted_army(army)?;
            let garrison = state.armies.get(&tutorial.active_run(state)?.garrison?)?;
            command(Command::SwapArmyMembers {
                a: garrison.id,
                b: named,
                a_members: garrison.roster.iter().map(|m| m.id).collect(),
                b_members: Vec::new(),
            })
        }
        Task::MoveHeroArmyToRegion { army, region } => command(Command::MoveArmy {
            army: state.scripted_army(army)?,
            dest: *region,
        }),
        Task::OpenDetailsPanel(_) => {
            let run = tutorial.active_run(state)?;
            Some(Action::Ui(if run.details {
                UiAction::HudDetailsSignal
            } else {
                UiAction::SelectArmy(run.garrison?)
            }))
        }
        Task::SelectBattle(region) => Some(Action::Ui(UiAction::SelectBattle(*region))),
        Task::EnableAutoResolve(_) => command(Command::ChooseBattleMode {
            battle: tutorial.active_run(state)?.battle?,
            method: 1,
        }),
        Task::RetreatArmies => retreat_option(state, player).map(Action::Command),
        Task::Unhandled(_)
        | Task::ScriptedArmyMove { .. }
        | Task::ScriptedDelay { .. }
        | Task::EndTutorial => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Playthrough {
    pub frames: u64,
    pub finished: bool,
    pub hashes: Vec<u64>,
    pub tutorial: Vec<TutorialEvent>,
    pub refused: Vec<(Action, Blocked)>,
}

struct Clock<'a> {
    frames: &'a mut dyn FnMut(&str) -> u32,
    left: Vec<u32>,
}

impl SessionAudio for Clock<'_> {
    fn play(&mut self, name: &str) -> Option<u32> {
        let handle = u32::try_from(self.left.len()).ok()?;
        self.left.push((self.frames)(name));
        Some(handle)
    }

    fn playing(&self, handle: u32) -> bool {
        usize::try_from(handle)
            .ok()
            .and_then(|i| self.left.get(i))
            .is_some_and(|left| *left > 0)
    }
}

pub fn autoplay(
    tutorial: &mut Tutorial,
    state: &mut LivingWorldState,
    mut resolver: Option<&mut dyn AutoResolve>,
    audio_frames: &mut dyn FnMut(&str) -> u32,
    max_frames: u64,
) -> Playthrough {
    let mut run = Playthrough::default();
    let mut clock = Clock {
        frames: audio_frames,
        left: Vec::new(),
    };
    let record = |events: &[Event], run: &mut Playthrough| {
        run.hashes.extend(events.iter().filter_map(|e| match e {
            Event::PhaseChanged { hash, .. } => Some(*hash),
            _ => None,
        }));
    };
    while run.frames < max_frames && !tutorial.finished() {
        run.frames = run.frames.saturating_add(1);
        if tutorial.popup_open() {
            run.tutorial.extend(tutorial.popup_closed());
            break;
        }
        for left in &mut clock.left {
            *left = left.saturating_sub(1);
        }
        let Tick {
            tutorial: events,
            sim,
        } = tutorial.tick(state, reborrow(&mut resolver), &mut clock);
        record(&sim, &mut run);
        run.tutorial.extend(events);
        let Some(action) = next_action(tutorial, state) else {
            continue;
        };
        let player = Tutorial::local(state).unwrap_or(PlayerId(0));
        let result = match &action {
            Action::Command(c) => tutorial
                .apply(state, reborrow(&mut resolver), player, c.clone())
                .map(|events| record(&events, &mut run)),
            Action::Ui(ui) => tutorial
                .ui(state, *ui)
                .then_some(())
                .ok_or(Blocked::ByTutorial),
        };
        if let Err(why) = result {
            run.refused.push((action, why));
        }
    }
    run.finished = tutorial.finished();
    run
}
