#![allow(missing_docs)]

mod player;
mod script;

pub use player::{autoplay, next_action, Action, Playthrough};
pub use script::{scripted_setup, Task, TutorialError, FRAMES_PER_SECOND};

use crate::battle::AutoResolve;
use crate::engine::apply_with;
use crate::model::{
    ArmyId, BattleMode, Command, Event, Phase, PlayerId, PlayerKind, RegionId, Rejected,
};
use crate::state::LivingWorldState;
use mint_sage_ini::living_world::players::BuildingType;
use mint_sage_ini::living_world::LivingWorldData;
use script::{Block, SessionScript};

pub const COMPLETED_TITLE: &str = "WOTRTutorial:TutorialCompletedTitle";
pub const COMPLETED_TEXT: &str = "WOTRTutorial:TutorialCompletedText";

pub trait SessionAudio {
    fn play(&mut self, name: &str) -> Option<u32>;
    fn playing(&self, handle: u32) -> bool;
}

pub struct Silent;

impl SessionAudio for Silent {
    fn play(&mut self, _: &str) -> Option<u32> {
        None
    }

    fn playing(&self, _: u32) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TutorialEvent {
    SessionStarted {
        turn: u32,
        phase: Phase,
        index: usize,
        audio: Option<String>,
        text: Option<String>,
        clear_text: bool,
    },
    TaskBegan {
        task: Task,
        text: Option<String>,
    },
    SessionDone {
        turn: u32,
        phase: Phase,
        index: usize,
    },
    ScriptedMove {
        army: String,
        region: RegionId,
        result: Result<(), Rejected>,
    },
    CompletionPopup {
        title: String,
        text: String,
    },
    Ended,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Tick {
    pub tutorial: Vec<TutorialEvent>,
    pub sim: Vec<Event>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    ByTutorial,
    Rejected(Rejected),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    SelectArmy(ArmyId),
    DeselectArmy(ArmyId),
    HudDetailsSignal,
    HudDetailsClosed,
    SelectBattle(RegionId),
    ClickPlot {
        region: RegionId,
        plot: u8,
    },
    ClickBuilding {
        region: RegionId,
        plot: u8,
    },
    ProductionEntry {
        region: RegionId,
        plot: u8,
        entry: u8,
    },
}

#[derive(Debug, Clone, Copy)]
enum Effect {
    Nothing,
    Details,
    DetailsOff,
    Hud,
    HudOff,
    Flag,
}

#[derive(Debug, Clone)]
struct Running {
    session: usize,
    handle: Option<u32>,
    begun: bool,
    task: Option<Task>,
    flag: bool,
    details: bool,
    hud: bool,
    opened: bool,
    until: u64,
    complete: bool,
    garrison: Option<ArmyId>,
    plot: Option<u8>,
    battle: Option<u32>,
}

#[derive(Debug, Clone, Default)]
struct Cursor {
    next: usize,
    running: Option<Running>,
}

#[derive(Debug, Clone)]
pub struct Tutorial {
    blocks: Vec<Block>,
    cursors: Vec<Cursor>,
    frame: u64,
    passed: Vec<(u32, Phase)>,
    popup_open: bool,
    finished: bool,
    tray_allowed: bool,
}

impl Tutorial {
    pub fn new(data: &LivingWorldData, state: &LivingWorldState) -> Result<Self, TutorialError> {
        let blocks = script::read(data, state)?;
        let cursors = vec![Cursor::default(); blocks.len()];
        Ok(Self {
            blocks,
            cursors,
            frame: 0,
            passed: Vec::new(),
            popup_open: false,
            finished: false,
            tray_allowed: false,
        })
    }

    #[must_use]
    pub fn details_tray_allowed(&self) -> bool {
        self.finished || self.tray_allowed
    }

    #[must_use]
    pub fn finished(&self) -> bool {
        self.finished
    }

    #[must_use]
    pub fn popup_open(&self) -> bool {
        self.popup_open
    }

    pub fn popup_closed(&mut self) -> Option<TutorialEvent> {
        if !self.popup_open {
            return None;
        }
        self.popup_open = false;
        self.finished = true;
        Some(TutorialEvent::Ended)
    }

    fn block_at(&self, state: &LivingWorldState) -> Option<usize> {
        self.blocks
            .iter()
            .position(|b| b.turn == state.turn && b.phase == state.phase)
    }

    fn session(&self, block: usize, session: usize) -> Option<&SessionScript> {
        self.blocks.get(block)?.sessions.get(session)
    }

    fn local(state: &LivingWorldState) -> Option<PlayerId> {
        state
            .players
            .iter()
            .find(|p| p.kind == PlayerKind::Human)
            .map(|p| p.id)
    }

    fn done(&self, run: &Running, state: &LivingWorldState) -> bool {
        let Some(task) = &run.task else { return true };
        match task {
            Task::Unhandled(_) | Task::EndTutorial => false,
            Task::ScriptedArmyMove { .. } => true,
            Task::ScriptedDelay { .. } => self.frame >= run.until,
            Task::BuildBarracks(_)
            | Task::TrainUnit(_)
            | Task::MergeGarrison { .. }
            | Task::SelectBattle(_) => run.flag,
            Task::OpenDetailsPanel(_) => run.opened,
            Task::MoveHeroArmyToRegion { army, region } => state
                .scripted_army(army)
                .and_then(|id| state.armies.get(&id))
                .is_some_and(|a| a.order == Some(*region)),
            Task::EnableAutoResolve(_) => run
                .battle
                .is_some_and(|id| !state.battles.iter().any(|b| b.id == id)),
            Task::RetreatArmies => {
                Self::local(state).is_none_or(|p| state.retreat_pending(p).is_empty())
            }
        }
    }

    fn active_run(&self, state: &LivingWorldState) -> Option<&Running> {
        let block = self.block_at(state)?;
        let run = self.cursors.get(block)?.running.as_ref()?;
        (run.begun && !run.complete && !self.done(run, state)).then_some(run)
    }

    #[must_use]
    pub fn active_task(&self, state: &LivingWorldState) -> Option<&Task> {
        self.active_run(state)?.task.as_ref()
    }

    #[must_use]
    pub fn task_text(&self, state: &LivingWorldState) -> Option<&str> {
        let block = self.block_at(state)?;
        let run = self.active_run(state)?;
        self.session(block, run.session)?
            .task
            .as_ref()?
            .text
            .as_deref()
    }

    #[must_use]
    pub fn exhausted(&self, state: &LivingWorldState) -> bool {
        match self.block_at(state) {
            None => true,
            Some(block) => self.cursors.get(block).is_some_and(|c| {
                c.running.is_none()
                    && c.next >= self.blocks.get(block).map_or(0, |b| b.sessions.len())
            }),
        }
    }

    #[must_use]
    pub fn battle_methods(&self, state: &LivingWorldState) -> u8 {
        match self.active_task(state) {
            Some(Task::EnableAutoResolve(_)) => 1 << 1,
            _ => 0,
        }
    }

    #[must_use]
    pub fn battle_selectable(&self, state: &LivingWorldState, region: RegionId) -> bool {
        self.finished
            || matches!(
                self.active_task(state),
                Some(Task::SelectBattle(r) | Task::EnableAutoResolve(r)) if *r == region
            )
    }

    fn method_offered(&self, state: &LivingWorldState, method: u8) -> bool {
        1u8.checked_shl(u32::from(method))
            .is_some_and(|bit| self.battle_methods(state) & bit != 0)
    }

    fn start_session(
        &mut self,
        block: usize,
        audio: &mut dyn SessionAudio,
        out: &mut Vec<TutorialEvent>,
    ) -> bool {
        let Some(cursor) = self.cursors.get_mut(block) else {
            return false;
        };
        let index = cursor.next;
        let Some(b) = self.blocks.get(block) else {
            return false;
        };
        let Some(session) = b.sessions.get(index) else {
            return false;
        };
        cursor.next = index.saturating_add(1);
        cursor.running = Some(Running {
            session: index,
            handle: session.audio.as_deref().and_then(|name| audio.play(name)),
            begun: false,
            task: session.task.as_ref().map(|t| t.task.clone()),
            flag: false,
            details: false,
            hud: false,
            opened: false,
            until: 0,
            complete: false,
            garrison: None,
            plot: None,
            battle: None,
        });
        out.push(TutorialEvent::SessionStarted {
            turn: b.turn,
            phase: b.phase,
            index,
            audio: session.audio.clone(),
            text: session.text.clone(),
            clear_text: session.clear_previous_text,
        });
        true
    }

    fn note_phases(&mut self, events: &[Event]) {
        for event in events {
            if let Event::PhaseChanged { turn, phase, .. } = event {
                self.passed.push((*turn, *phase));
            }
        }
    }

    fn replay_passed(
        &mut self,
        state: &LivingWorldState,
        audio: &mut dyn SessionAudio,
        out: &mut Vec<TutorialEvent>,
    ) {
        let passed = std::mem::take(&mut self.passed);
        for (turn, phase) in passed {
            if (turn, phase) == (state.turn, state.phase) {
                continue;
            }
            let Some(block) = self
                .blocks
                .iter()
                .position(|b| b.turn == turn && b.phase == phase)
            else {
                continue;
            };
            if self.cursors.get(block).is_some_and(|c| c.running.is_none())
                && self.start_session(block, audio, out)
            {
                if let Some(c) = self.cursors.get_mut(block) {
                    c.running = None;
                }
            }
        }
    }

    pub fn tick(
        &mut self,
        state: &mut LivingWorldState,
        resolver: Option<&mut dyn AutoResolve>,
        audio: &mut dyn SessionAudio,
    ) -> Tick {
        self.frame = self.frame.saturating_add(1);
        let mut out = Tick::default();
        if self.finished {
            return out;
        }
        self.replay_passed(state, audio, &mut out.tutorial);
        let current = self.block_at(state);
        for (i, cursor) in self.cursors.iter_mut().enumerate() {
            if Some(i) != current {
                cursor.running = None;
            }
        }
        let Some(block) = current else { return out };
        if self.cursors.get(block).is_some_and(|c| c.running.is_none())
            && !self.start_session(block, audio, &mut out.tutorial)
        {
            return out;
        }
        let Some(mut run) = self.cursors.get(block).and_then(|c| c.running.clone()) else {
            return out;
        };
        let playing = run.handle.is_some_and(|h| audio.playing(h));
        if !playing && !run.begun {
            run.begun = true;
            self.begin(block, &mut run, state, resolver, &mut out);
        }
        if run.begun && !run.complete {
            run.complete = self.done(&run, state);
            if run.complete && matches!(run.task, Some(Task::EnableAutoResolve(_))) {
                self.tray_allowed = true;
            }
        }
        let finished_session = run.begun && run.complete && !playing;
        if let Some(cursor) = self.cursors.get_mut(block) {
            cursor.running = if finished_session {
                None
            } else {
                Some(run.clone())
            };
        }
        if finished_session {
            if let Some(b) = self.blocks.get(block) {
                out.tutorial.push(TutorialEvent::SessionDone {
                    turn: b.turn,
                    phase: b.phase,
                    index: run.session,
                });
            }
            self.start_session(block, audio, &mut out.tutorial);
        }
        out
    }

    fn resolve(run: &mut Running, task: &Task, state: &LivingWorldState) {
        let garrison_in = |region: RegionId| {
            let local = Self::local(state)?;
            state
                .armies_of(local)
                .find(|g| !g.is_hero_army() && g.region == region)
                .map(|g| g.id)
        };
        match task {
            Task::MergeGarrison { region, .. } | Task::OpenDetailsPanel(region) => {
                run.garrison = garrison_in(*region);
            }
            Task::TrainUnit(region) => {
                run.plot = state.region(*region).and_then(|r| {
                    let at = r.plots.iter().position(|p| {
                        p.as_ref().is_some_and(|p| {
                            state.rules.building(p.building).and_then(|b| b.kind)
                                == Some(BuildingType::Barracks)
                        })
                    })?;
                    u8::try_from(at).ok()
                });
            }
            Task::EnableAutoResolve(region) => {
                run.battle = state
                    .battles
                    .iter()
                    .find(|b| b.region == *region)
                    .map(|b| b.id);
            }
            _ => {}
        }
    }

    fn begin(
        &mut self,
        block: usize,
        run: &mut Running,
        state: &mut LivingWorldState,
        resolver: Option<&mut dyn AutoResolve>,
        out: &mut Tick,
    ) {
        let Some(task) = run.task.clone() else { return };
        let text = self
            .session(block, run.session)
            .and_then(|s| s.task.as_ref())
            .and_then(|t| t.text.clone());
        out.tutorial.push(TutorialEvent::TaskBegan {
            task: task.clone(),
            text,
        });
        Self::resolve(run, &task, state);
        match task {
            Task::ScriptedDelay { frames } => {
                run.until = self.frame.saturating_add(u64::from(frames));
            }
            Task::ScriptedArmyMove { army, region } => {
                let result = match state.scripted_army(&army) {
                    Some(id) => {
                        let owner = state.armies.get(&id).map(|a| a.player);
                        match owner {
                            Some(owner) => {
                                match apply_with(
                                    state,
                                    resolver,
                                    owner,
                                    Command::MoveArmy {
                                        army: id,
                                        dest: region,
                                    },
                                ) {
                                    Ok(events) => {
                                        self.note_phases(&events);
                                        out.sim.extend(events);
                                        Ok(())
                                    }
                                    Err(why) => Err(why),
                                }
                            }
                            None => Err(Rejected::NoSuchArmy),
                        }
                    }
                    None => Err(Rejected::NoSuchArmy),
                };
                out.tutorial.push(TutorialEvent::ScriptedMove {
                    army,
                    region,
                    result,
                });
            }
            Task::OpenDetailsPanel(_) => self.tray_allowed = true,
            Task::EndTutorial => {
                self.popup_open = true;
                out.tutorial.push(TutorialEvent::CompletionPopup {
                    title: COMPLETED_TITLE.to_owned(),
                    text: COMPLETED_TEXT.to_owned(),
                });
            }
            _ => {}
        }
    }

    fn judge(&self, state: &LivingWorldState, command: &Command) -> Option<bool> {
        let run = self.active_run(state);
        let task = run.and_then(|r| r.task.as_ref());
        match command {
            Command::DisbandMember { .. }
            | Command::DisbandAll { .. }
            | Command::CancelQueuedArmy { .. }
            | Command::Demolish { .. } => None,
            Command::SwapArmyMembers {
                a, b, a_members, ..
            } => match task? {
                Task::MergeGarrison { army, .. } => {
                    let named = state.scripted_army(army)?;
                    let garrison = run?.garrison?;
                    if (*a, *b) != (garrison, named) {
                        return None;
                    }
                    let held = state.armies.get(&garrison)?;
                    Some(
                        a_members
                            .iter()
                            .any(|id| held.roster.iter().any(|m| m.id == *id)),
                    )
                }
                _ => None,
            },
            Command::MoveArmy { army, dest } => match task? {
                Task::MoveHeroArmyToRegion { army: name, region } => {
                    (state.scripted_army(name) == Some(*army) && dest == region).then_some(true)
                }
                _ => None,
            },
            Command::CancelMove { .. } => {
                matches!(task?, Task::MoveHeroArmyToRegion { .. }).then_some(false)
            }
            Command::ChooseRetreat { .. } => matches!(task?, Task::RetreatArmies).then_some(false),
            Command::Build {
                region, building, ..
            } => match task? {
                Task::BuildBarracks(target) if target == region => Some(
                    state.rules.building(*building).and_then(|b| b.kind)
                        == Some(BuildingType::Barracks),
                ),
                _ => None,
            },
            Command::QueueArmy { region, plot, .. } => match task? {
                Task::TrainUnit(target) => {
                    (target == region && run?.plot == Some(*plot)).then_some(true)
                }
                _ => None,
            },
            Command::ChooseBattleMode { method, .. } => {
                self.method_offered(state, *method).then_some(false)
            }
            Command::ResolveBattle { mode, .. } => {
                (*mode == BattleMode::Auto && self.method_offered(state, 1)).then_some(false)
            }
            Command::ForceRealTime { .. } => None,
            _ => Some(false),
        }
    }

    pub fn apply(
        &mut self,
        state: &mut LivingWorldState,
        resolver: Option<&mut dyn AutoResolve>,
        player: PlayerId,
        command: Command,
    ) -> Result<Vec<Event>, Blocked> {
        let human = !self.finished && state.kind(player) == Some(PlayerKind::Human);
        let completes = if human {
            self.judge(state, &command).ok_or(Blocked::ByTutorial)?
        } else {
            false
        };
        let block = self.block_at(state);
        let events = apply_with(state, resolver, player, command).map_err(Blocked::Rejected)?;
        self.note_phases(&events);
        if completes {
            if let Some(run) = block
                .and_then(|b| self.cursors.get_mut(b))
                .and_then(|c| c.running.as_mut())
            {
                run.flag = true;
            }
        }
        if let Some(run) = block
            .and_then(|b| self.cursors.get(b))
            .and_then(|c| c.running.as_ref())
        {
            if run.begun
                && matches!(run.task, Some(Task::EnableAutoResolve(_)))
                && self.done(run, state)
            {
                self.tray_allowed = true;
            }
        }
        Ok(events)
    }

    fn offered(state: &LivingWorldState, region: RegionId, plot: u8, entry: u8) -> bool {
        state
            .region(region)
            .and_then(|r| r.plots.get(usize::from(plot))?.as_ref())
            .and_then(|p| state.rules.building(p.building)?.queue.as_ref())
            .and_then(|q| q.entries.get(usize::from(entry)))
            .is_some_and(|e| e.build_time == 1)
    }

    fn effect_of(&self, state: &LivingWorldState, action: UiAction) -> Option<Effect> {
        let run = self.active_run(state)?;
        let task = run.task.as_ref()?;
        match (task, action) {
            (Task::OpenDetailsPanel(_), UiAction::SelectArmy(a)) if run.garrison == Some(a) => {
                Some(Effect::Details)
            }
            (Task::MergeGarrison { .. }, UiAction::SelectArmy(a)) if run.garrison == Some(a) => {
                Some(Effect::Nothing)
            }
            (Task::MoveHeroArmyToRegion { army, .. }, UiAction::SelectArmy(a))
                if state.scripted_army(army) == Some(a) =>
            {
                Some(Effect::Nothing)
            }
            (Task::RetreatArmies, UiAction::SelectArmy(a))
                if state.retreat_entries.contains(&a)
                    && state.armies.get(&a).is_some_and(|army| {
                        army.is_hero_army() && Some(army.player) == Self::local(state)
                    }) =>
            {
                Some(Effect::Nothing)
            }
            (Task::OpenDetailsPanel(_), UiAction::DeselectArmy(a)) if run.garrison == Some(a) => {
                Some(Effect::DetailsOff)
            }
            (Task::OpenDetailsPanel(_), UiAction::HudDetailsSignal) => Some(Effect::Hud),
            (Task::OpenDetailsPanel(_), UiAction::HudDetailsClosed) => Some(Effect::HudOff),
            (Task::SelectBattle(r), UiAction::SelectBattle(c)) if *r == c => Some(Effect::Flag),
            (Task::BuildBarracks(r), UiAction::ClickPlot { region, .. }) if *r == region => {
                Some(Effect::Nothing)
            }
            (Task::TrainUnit(r), UiAction::ClickBuilding { region, plot })
                if *r == region && run.plot == Some(plot) =>
            {
                Some(Effect::Nothing)
            }
            (
                Task::TrainUnit(r),
                UiAction::ProductionEntry {
                    region,
                    plot,
                    entry,
                },
            ) if *r == region
                && run.plot == Some(plot)
                && Self::offered(state, region, plot, entry) =>
            {
                Some(Effect::Nothing)
            }
            _ => None,
        }
    }

    pub fn ui(&mut self, state: &LivingWorldState, action: UiAction) -> bool {
        let ungated = matches!(
            action,
            UiAction::DeselectArmy(_) | UiAction::HudDetailsClosed
        );
        let Some(effect) = self.effect_of(state, action) else {
            return ungated;
        };
        if let Some(run) = self
            .block_at(state)
            .and_then(|b| self.cursors.get_mut(b))
            .and_then(|c| c.running.as_mut())
        {
            match effect {
                Effect::Nothing => {}
                Effect::Details => run.details = true,
                Effect::DetailsOff => run.details = false,
                Effect::Hud => run.hud = true,
                Effect::HudOff => run.hud = false,
                Effect::Flag => run.flag = true,
            }
            if run.details && run.hud {
                run.opened = true;
            }
        }
        true
    }
}
