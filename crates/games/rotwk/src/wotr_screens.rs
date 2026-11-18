use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

use mint_sage_apt_player::Value;
use mint_sage_livingworld::tutorial::{Task, TutorialEvent, UiAction};
use mint_sage_livingworld::{
    ArmyId, Battle, Command, LivingWorldState, Phase, PlayerId, RegionId, Rejected,
};

use crate::hud::{printed, ImageSource};
use crate::wotr_dialog::{plan, Call, Event, Out, Playback, Post};
use crate::wotr_hud::{
    commands, describe, garrison_target, intent_named, intent_of, ClipEvent, Intent, Refused,
    Selection,
};
use crate::wotr_movies::{Kind, Movies, Sent};
use crate::wotr_narration as narration;
use crate::wotr_resolve::ResolvedBattle;

mod award;
mod cache;
mod checklist;
mod command_ui;
mod config;
mod end_turn;
mod forward;
#[cfg(test)]
mod hosting_tests;
mod message;
#[cfg(test)]
mod panel_tests;
mod panels;
mod prompt;
mod retinue;
mod stats;
mod swapper;
#[cfg(test)]
mod tests;
mod veterancy;

const NARRATION_SECONDS: f32 = 1_000_000.0;

use cache::StateCache;
use command_ui::Frames;
pub(crate) use config::HudConfig;
use end_turn::EndTurn;
use forward::Signal;
pub(crate) use message::MessageSource;
use message::{Message, MessageQueue};
use prompt::{Side, Tabs};
pub(crate) use veterancy::Charge;

pub(crate) trait Strategic {
    fn state(&self) -> &LivingWorldState;
    fn submit(&mut self, player: PlayerId, command: Command) -> Result<(), Refused>;
    fn resolved(&self) -> Option<&ResolvedBattle>;
    fn ui(&mut self, _action: UiAction) -> bool {
        true
    }
    fn battle_selectable(&self, _region: RegionId) -> bool {
        true
    }
    fn tray_allowed(&self) -> Option<bool> {
        None
    }
    fn take_charges(&mut self) -> Vec<veterancy::Charge> {
        Vec::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogState {
    Opening,
    Open,
    Fading,
}

struct DialogRun {
    battle: u32,
    region: RegionId,
    playback: Playback,
    queue: VecDeque<(Call, u32)>,
    state: DialogState,
}

const RETRY_FRAMES: u32 = 300;

struct PromptRun {
    battle: u32,
    pages: Vec<prompt::Page>,
    heroes: usize,
    added: bool,
    tabs: [u8; 2],
    ally: Tabs,
    enemy: Tabs,
}

struct VeterancyRun {
    rows: Vec<veterancy::Row>,
    marked: BTreeSet<usize>,
}

struct AwardRun {
    region: RegionId,
    players: Vec<PlayerId>,
    counted: bool,
    filled: bool,
}

struct PendingCall {
    kind: Kind,
    path: String,
    function: String,
    args: Vec<Value>,
    tries: u32,
    cache_key: Option<String>,
}

pub(crate) struct Hud {
    pub(crate) movies: Movies,
    pub(crate) selection: Selection,
    pub(crate) local: PlayerId,
    pub(crate) ctrl: bool,
    config: HudConfig,
    dialog: Option<DialogRun>,
    calls: VecDeque<PendingCall>,
    cache: StateCache,
    end_turn: EndTurn,
    prompt: Option<PromptRun>,
    award: Option<AwardRun>,
    veterancy: Option<VeterancyRun>,
    veterancy_due: Option<RegionId>,
    frames: Frames,
    attached: BTreeMap<usize, command_ui::Button>,
    tray: Option<panels::Tray>,
    tray_open: bool,
    pub(crate) tutorial_tray: Option<bool>,
    tray_lock: panels::TrayLock,
    told: forward::Told,
    messages: MessageQueue,
    retinue: retinue::RetinueView,
    swapper: Option<swapper::SwapperRun>,
    published: BTreeMap<String, String>,
    ready: bool,
    checklist: checklist::Checklist,
    stats: stats::Stats,
    played: Option<u32>,
    pub(crate) lines: Vec<String>,
}

impl Hud {
    pub(crate) fn new(movies: Movies, local: PlayerId, config: HudConfig) -> Self {
        let mut hud = Self {
            movies,
            selection: Selection::default(),
            local,
            ctrl: false,
            config,
            dialog: None,
            calls: VecDeque::new(),
            cache: StateCache::default(),
            end_turn: EndTurn::default(),
            prompt: None,
            award: None,
            veterancy: None,
            veterancy_due: None,
            frames: Frames::default(),
            attached: BTreeMap::new(),
            tray: None,
            tray_open: false,
            tutorial_tray: None,
            tray_lock: panels::TrayLock::default(),
            told: forward::Told::default(),
            messages: MessageQueue::default(),
            retinue: retinue::RetinueView::at(panels::TRAY),
            swapper: None,
            published: BTreeMap::new(),
            ready: false,
            checklist: checklist::Checklist::default(),
            stats: stats::Stats::default(),
            played: None,
            lines: Vec::new(),
        };
        hud.seed_wrappers();
        hud
    }

    fn seed_wrappers(&mut self) {
        self.cache.seed(
            Kind::Hud,
            end_turn::CLIP,
            "SetState",
            end_turn::state_label(true),
        );
    }

    pub(crate) fn say(&mut self, line: String) {
        println!("{line}");
        self.lines.push(line);
    }

    pub(crate) fn run_intent<S: Strategic>(&mut self, session: &mut S, intent: &Intent) {
        if *intent == Intent::EndTurn && !end_turn::enabled(session.state().phase) {
            self.say(format!(
                "hud: end turn is disabled in {:?}",
                session.state().phase
            ));
            return;
        }
        match commands(session.state(), &self.selection, intent) {
            Err(why) => self.say(format!("hud: {intent:?} -> not available: {why:?}")),
            Ok(list) if list.is_empty() => self.say(format!("hud: {intent:?} -> no command")),
            Ok(list) => {
                for command in list {
                    let result = session.submit(self.local, command.clone());
                    let line = describe(self.local, &command, &result);
                    self.say(line);
                }
            }
        }
    }

    pub(crate) fn clip_event<S: Strategic>(&mut self, session: &mut S, sent: &Sent) {
        let suffix = sent.event.suffix.as_str();
        match (sent.kind, suffix) {
            (Kind::Veterancy, _) => {
                self.veterancy_event(session, suffix, &sent.event.args);
                return;
            }
            (Kind::Swapper, _) => {
                self.swapper_event(session, &sent.event);
                return;
            }
            (Kind::Hud, s) if s.starts_with(retinue::SLOT_LOADED) => {
                let mut view = std::mem::take(&mut self.retinue);
                if view.holds(&sent.event.clip) {
                    self.slot_loaded(Kind::Hud, &mut view, s, &sent.event.args);
                }
                self.retinue = view;
                return;
            }
            (Kind::Hud, "_OnButtonFrameLoaded") => {
                if let Some(index) = self.frames.loaded(&sent.event.args) {
                    self.say(format!("hud: command frame {index} loaded"));
                }
                return;
            }
            (Kind::Hud, "_OnChecklistLoaded") => {
                self.checklist.loaded(&sent.event.args);
                return;
            }
            (Kind::Hud, "_OnStatsDisplayLoaded") => {
                self.stats.loaded(&sent.event.args);
                return;
            }
            (Kind::Hud, "_OnInitialized") if sent.event.clip == "StrategicHUD" => {
                self.ready = true;
                return;
            }
            (Kind::Hud, "_OnOpened" | "_OnClosed") if sent.event.name() == "selectiondetails" => {
                self.tray_open = suffix == "_OnOpened";
                let action = if self.tray_open {
                    UiAction::HudDetailsSignal
                } else {
                    UiAction::HudDetailsClosed
                };
                let allowed = session.ui(action);
                self.say(format!(
                    "hud: details tray {} (told to the tutorial: {allowed})",
                    if self.tray_open { "opened" } else { "closed" }
                ));
                return;
            }
            (Kind::Message, _) => {
                self.say(format!(
                    "hud: message box sent {}{}",
                    sent.event.clip, sent.event.suffix
                ));
                if message::is_button_command(&sent.event.clip, suffix) {
                    self.close_message();
                }
                return;
            }
            (Kind::Hud, "_OnClicked") => {
                let built = self
                    .frames
                    .index_of(&sent.event.clip)
                    .and_then(|index| self.attached.get(&index));
                if let Some(command_ui::Button::Build(building)) = built.copied() {
                    self.run_intent(session, &Intent::BuildOnPlot(building));
                    return;
                }
            }
            (Kind::Dialog, "_OnAllyPanelLoaded" | "_OnEnemyPanelLoaded") => {
                let panel = if suffix == "_OnAllyPanelLoaded" {
                    crate::wotr_dialog::Panel::Allies
                } else {
                    crate::wotr_dialog::Panel::Enemies
                };
                let index = command_ui::arg_field(&sent.event.args, "index")
                    .and_then(|index| index.parse().ok());
                if let Some(index) = index {
                    let outs = self
                        .dialog
                        .as_mut()
                        .map(|d| d.playback.panel_loaded(panel, index))
                        .unwrap_or_default();
                    self.dialog_outs(session, outs);
                }
                return;
            }
            (Kind::Prompt, "_OnTabClicked") => {
                self.tab_clicked(&sent.event);
                return;
            }
            _ => {}
        }
        let event = self.translated(sent);
        let Some(intent) = intent_of(&event) else {
            self.say(format!(
                "hud: {} {}{} -> no meaning known",
                sent.kind.movie(),
                sent.event.clip,
                sent.event.suffix
            ));
            return;
        };
        let dialog = sent.kind == Kind::Dialog;
        match intent {
            Intent::Presentation => {}
            Intent::ToggleDetails => self.toggle_details(),
            Intent::LeaderIcon => self.selection.leader_clicked(session.state()),
            Intent::IconListBackground => self.selection.background_clicked(),
            Intent::PickMember(member) => self.selection.member_clicked(member, self.ctrl),
            Intent::PickAwardRow(row) => self.award_row_clicked(row),
            Intent::PickArmy(slot) => self.pick_army(session, slot),
            Intent::PickPlot(slot) => self.pick_plot(session, slot),
            Intent::DialogHitDone if dialog => self.dialog_event(session, Event::HitDone),
            Intent::DialogChain if dialog => self.dialog_event(session, Event::Chain),
            Intent::DialogReinforceDone if dialog => {
                self.dialog_event(session, Event::ReinforceDone);
            }
            Intent::Opened if dialog => self.dialog_opened(),
            Intent::Closed if dialog => self.dialog_closed(),
            Intent::Opened | Intent::Closed => {}
            Intent::DialogSkip => self.dialog_skip(session),
            other => self.run_intent(session, &other),
        }
    }

    fn sync<S: Strategic>(&mut self, session: &S) {
        let state = session.state();
        let prompt = (state.phase == Phase::ResolveBattles && state.awaiting.is_none())
            .then(|| {
                state.battles.iter().find(|b| {
                    b.players.contains(&self.local)
                        && !b.choices.iter().any(|(p, _)| *p == self.local)
                        && session.battle_selectable(b.region)
                })
            })
            .flatten();
        self.selection.battle = prompt.map(|b| b.id);
        let award = state
            .decisions
            .values()
            .find(|d| !d.resolved && state.decision_chooser(d) == Some(self.local));
        if let Some(decision) = award {
            self.selection.region = Some(decision.region);
        }
        let dialog = self.dialog.is_some();
        self.show(Kind::Dialog, dialog);
        let award_up = award.is_some() && !dialog;
        let prompt_up = prompt.is_some() && !dialog && self.veterancy.is_none();
        let award_opens = award_up && !self.movies.is_up(Kind::Award);
        let prompt_opens = prompt_up && !self.movies.is_up(Kind::Prompt);
        self.show(Kind::Award, award_up);
        self.show(Kind::Prompt, prompt_up);
        if let (true, Some(decision)) = (award_opens, award) {
            self.award = Some(AwardRun {
                region: decision.region,
                players: decision.players.clone(),
                counted: false,
                filled: false,
            });
        }
        if prompt_opens {
            for button in ["AutoResolve", "RealTime", "Retreat"] {
                self.movies
                    .set_value(Kind::Prompt, "", &format!("{button}State"), "_up");
            }
            if let Some(battle) = prompt {
                self.open_prompt(state, battle);
            }
        }
        if !prompt_up {
            self.prompt = None;
        }
        if !award_up {
            self.award = None;
        }
    }

    pub(crate) fn veterancy_follows(&mut self, region: RegionId) {
        self.veterancy_due = Some(region);
    }

    fn open_veterancy<S: Strategic>(&mut self, session: &mut S) {
        if self.dialog.is_some() {
            return;
        }
        let Some(region) = self.veterancy_due.take() else {
            return;
        };
        let charges = session.take_charges();
        let state = session.state();
        let rows = veterancy::rows(state, self.local, region, &charges);
        if rows.is_empty() {
            return;
        }
        let treasury = state.player(self.local).map_or(0, |p| p.resources);
        self.show(Kind::Veterancy, true);
        let externs = [
            ("NumRows", rows.len().to_string()),
            ("TreasuryBalance", treasury.to_string()),
            (
                "UnitChargeList",
                veterancy::list(&rows, |r| r.charge.unwrap_or(0).to_string()),
            ),
            (
                "UnitNewFlags",
                veterancy::list(&rows, |r| u8::from(r.charge.is_some()).to_string()),
            ),
            (
                "UnitHeroFlags",
                veterancy::list(&rows, |r| u8::from(r.hero).to_string()),
            ),
            (
                "UnitUpgradeFlags",
                veterancy::list(&rows, |r| u8::from(r.upgraded).to_string()),
            ),
        ];
        if let Some(movie) = self.movies.hosted(Kind::Veterancy) {
            for (name, value) in &externs {
                movie
                    .expose
                    .set_extern(&format!("StrategicVeterancy::{name}"), Value::str(value));
            }
        }
        for (index, row) in rows.iter().enumerate() {
            let rank = self
                .movies
                .formatted("APT:RankLabel", i64::from(row.level))
                .unwrap_or_else(|| row.level.to_string());
            let counts = [row.battles, row.kills_battle, row.kills].map(|n| n.to_string());
            for (field, text) in [
                ("UnitName", row.name.as_str()),
                ("UnitLevel", rank.as_str()),
                ("UnitBattles", counts[0].as_str()),
                ("UnitKillsInBattle", counts[1].as_str()),
                ("UnitKillsInWar", counts[2].as_str()),
            ] {
                let key = format!("StrategicVeterancy:{field}_{index}");
                self.movies.set_text(Kind::Veterancy, &key, text);
            }
            self.say(format!(
                "veterancy row {index}: {} level {} battles {} kills {} (war {}) picture {:?}",
                row.name, row.level, row.battles, row.kills_battle, row.kills, row.picture
            ));
            self.movies.set_image(
                Kind::Veterancy,
                &format!("StrategicVeterancy_UnitImage_{index}"),
                Self::face_image(row.picture.clone()),
            );
        }
        self.say(format!(
            "veterancy: {} row(s), {} new, treasury {treasury}",
            rows.len(),
            rows.iter().filter(|r| r.charge.is_some()).count()
        ));
        self.veterancy = Some(VeterancyRun {
            rows,
            marked: BTreeSet::new(),
        });
    }

    fn veterancy_event<S: Strategic>(&mut self, session: &mut S, name: &str, args: &str) {
        match name {
            "MarkUnitForDisband" => {
                if let Some((row, on)) = veterancy::parse_mark(args) {
                    self.veterancy_mark(row, on);
                }
            }
            "OnContinue" => {
                let list = self
                    .veterancy
                    .as_ref()
                    .map_or_else(Vec::new, |run| veterancy::disbands(&run.rows, &run.marked));
                for command in list {
                    let result = session.submit(self.local, command.clone());
                    let line = describe(self.local, &command, &result);
                    self.say(line);
                }
            }
            "OnFadeOut" => self.close_veterancy(),
            "ShowTreasuryWarning" => self.say("veterancy: the treasury is below 0".to_owned()),
            _ => {}
        }
    }

    pub(crate) fn veterancy_mark(&mut self, row: usize, on: bool) {
        if let Some(run) = self.veterancy.as_mut() {
            if on {
                run.marked.insert(row);
            } else {
                run.marked.remove(&row);
            }
        }
    }

    pub(crate) fn close_veterancy(&mut self) {
        self.veterancy = None;
        self.movies.show(Kind::Veterancy, false);
    }

    pub(crate) fn veterancy_report(&self) -> String {
        self.veterancy.as_ref().map_or_else(
            || "no veterancy screen".to_owned(),
            |run| {
                run.rows
                    .iter()
                    .map(|r| {
                        format!(
                            "{} rank {} {} charge {:?}",
                            r.name,
                            r.level,
                            if r.hero { "hero" } else { "unit" },
                            r.charge
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            },
        )
    }

    fn show(&mut self, kind: Kind, up: bool) {
        let was = self.movies.is_up(kind);
        self.movies.show(kind, up);
        if up && !was {
            self.cache.clear(kind);
            if kind == Kind::Hud {
                self.seed_wrappers();
            }
        }
    }

    fn start_dialog<S: Strategic>(&mut self, session: &mut S) {
        let Some(resolved) = session.resolved() else {
            return;
        };
        if self.played == Some(resolved.battle) || self.dialog.is_some() {
            return;
        }
        let Some(plan) = plan(resolved, self.local) else {
            return;
        };
        let battle = resolved.battle;
        let region = resolved.region;
        let region_text = session
            .state()
            .battles
            .iter()
            .find(|b| b.id == battle)
            .and_then(|b| session.state().rules.region_rules(b.region))
            .map(|r| r.name.clone());
        self.played = Some(battle);
        let rounds = plan.rounds;
        self.say(format!(
            "dialog: battle {battle} opens, {rounds} round(s), {} participant(s), {} round(s) per step",
            plan.entries.len(),
            self.config.rounds_per_step()
        ));
        if let Some(name) = region_text {
            self.movies
                .set_text(Kind::Dialog, "_level0_RegionName", &name);
        }
        let (playback, outs) = Playback::start(plan, self.config.rounds_per_step());
        self.dialog = Some(DialogRun {
            battle,
            region,
            playback,
            queue: VecDeque::new(),
            state: DialogState::Opening,
        });
        self.show(Kind::Dialog, true);
        self.dialog_outs(session, outs);
    }

    fn dialog_outs<S: Strategic>(&mut self, session: &mut S, outs: Vec<Out>) {
        for out in outs {
            self.say(format!("dialog: {out:?}"));
            match out {
                Out::Call(call) => {
                    if let Some(run) = self.dialog.as_mut() {
                        run.queue.push_back((call, 0));
                    }
                }
                Out::Post(post) => {
                    let Some(battle) = self.dialog.as_ref().map(|d| d.battle) else {
                        continue;
                    };
                    let command = match post {
                        Post::Step { target } => Command::AutoResolveStep {
                            battle,
                            round: target,
                        },
                        Post::Skip => Command::AutoResolveSkip { battle },
                    };
                    let result = session.submit(self.local, command.clone());
                    let line = describe(self.local, &command, &result);
                    self.say(line);
                }
            }
        }
    }

    fn dialog_event<S: Strategic>(&mut self, session: &mut S, event: Event) {
        let outs = self
            .dialog
            .as_mut()
            .filter(|d| d.state != DialogState::Fading)
            .map(|d| d.playback.event(event))
            .unwrap_or_default();
        self.dialog_outs(session, outs);
    }

    fn dialog_opened(&mut self) {
        if let Some(run) = self
            .dialog
            .as_mut()
            .filter(|d| d.state == DialogState::Opening)
        {
            run.state = DialogState::Open;
        }
    }

    fn dialog_closed(&mut self) {
        if self
            .dialog
            .as_ref()
            .is_some_and(|d| d.state == DialogState::Fading)
        {
            self.say("dialog: closed by the movie".to_owned());
            self.close_dialog();
        }
    }

    fn dialog_skip<S: Strategic>(&mut self, session: &mut S) {
        let outs = self
            .dialog
            .as_mut()
            .filter(|d| d.state == DialogState::Open)
            .map(|d| d.playback.skip())
            .unwrap_or_default();
        self.dialog_outs(session, outs);
    }

    fn apply(&mut self, call: &Call) -> Result<(), String> {
        let number = |n: u32| Value::Number(f64::from(n));
        let float = |n: f32| Value::Number(f64::from(n));
        let slot_key = |slot: crate::wotr_dialog::Slot| printed(&slot.path());
        match call {
            Call::NumPlayers(panel, n) => {
                let (group, function) = match panel {
                    crate::wotr_dialog::Panel::Allies => ("main.allies", "SetNumAllies"),
                    crate::wotr_dialog::Panel::Enemies => ("main.enemies", "SetNumEnemies"),
                };
                if !self.movies.exists(Kind::Dialog, group) {
                    return Err(format!("no clip {group}"));
                }
                self.movies
                    .call(Kind::Dialog, "", function, vec![number(*n)])
            }
            Call::Identity(i) => {
                let Some((slot, player)) = self
                    .dialog
                    .as_ref()
                    .and_then(|d| d.playback.entry(*i))
                    .map(|e| (e.slot, e.player))
                else {
                    return Ok(());
                };
                let face = self.config.face(player);
                let key = slot_key(slot);
                self.movies
                    .set_text(Kind::Dialog, &format!("{key}_PlayerName"), &face.name);
                if let Some(icon) = face.faction_icon {
                    self.movies.set_image(
                        Kind::Dialog,
                        &format!("{key}_PlayerIcon"),
                        ImageSource::MappedImage(icon),
                    );
                }
                self.movies.call(
                    Kind::Dialog,
                    &slot.path(),
                    "SetPlayerColor",
                    vec![Value::Number(f64::from(face.color))],
                )
            }
            Call::Health(slot, pct) => self.movies.call(
                Kind::Dialog,
                &slot.path(),
                "SetPlayerHealth",
                vec![float(*pct)],
            ),
            Call::Hit(slot, pct) => {
                self.movies
                    .call(Kind::Dialog, &slot.path(), "PlayHitAnim", vec![float(*pct)])
            }
            Call::Reinforce(slot, pct) => self.movies.call(
                Kind::Dialog,
                &slot.path(),
                "PlayReinforceAnim",
                vec![float(*pct)],
            ),
            Call::UnitCount(slot, n) => {
                self.movies.set_text(
                    Kind::Dialog,
                    &format!("{}_UnitCount", slot_key(*slot)),
                    &n.to_string(),
                );
                Ok(())
            }
            Call::Banner(slot, index, visible) => {
                let hero = self.dialog.as_ref().and_then(|d| {
                    d.playback
                        .plan()
                        .entries
                        .iter()
                        .find(|e| e.slot == *slot)
                        .and_then(|e| e.banner_hero(*index))
                        .map(str::to_owned)
                });
                let icon = self.config.banner_icon(hero.as_deref()).map(str::to_owned);
                self.say(format!(
                    "dialog: banner {} #{index} hero {hero:?} -> {icon:?}",
                    slot.path()
                ));
                self.movies.set_image(
                    Kind::Dialog,
                    &format!("{}_Banner{index}", slot_key(*slot)),
                    Self::face_image(icon),
                );
                self.movies.call(
                    Kind::Dialog,
                    &slot.path(),
                    "SetBannerVisibility",
                    vec![number(*index), Value::Bool(*visible)],
                )
            }
            Call::State(slot, label) => self.movies.call(
                Kind::Dialog,
                &slot.path(),
                "SetState",
                vec![Value::str(label)],
            ),
            Call::Result(label) => {
                self.movies
                    .call(Kind::Dialog, "", "SetResultState", vec![Value::str(label)])
            }
            Call::FadeOut => {
                let result = self.movies.call(Kind::Dialog, "", "FadeOut", Vec::new());
                if result.is_ok() {
                    if let Some(run) = self.dialog.as_mut() {
                        run.state = DialogState::Fading;
                    }
                    let report = self.dialog_report();
                    self.say(format!("dialog: finish sent; last frame {report}"));
                }
                result
            }
        }
    }

    fn call(&mut self, kind: Kind, path: &str, function: &str, args: Vec<Value>) {
        self.calls.push_back(PendingCall {
            kind,
            path: path.to_owned(),
            function: function.to_owned(),
            args,
            tries: 0,
            cache_key: None,
        });
    }

    fn state_call(&mut self, kind: Kind, path: &str, function: &str, label: &str) {
        if self.cache.changed(kind, path, function, label) {
            self.calls.push_back(PendingCall {
                kind,
                path: path.to_owned(),
                function: function.to_owned(),
                args: vec![Value::str(label)],
                tries: 0,
                cache_key: Some(function.to_owned()),
            });
        }
    }

    fn pump_calls(&mut self) {
        for _ in 0..self.calls.len() {
            let Some(mut pending) = self.calls.pop_front() else {
                break;
            };
            if !self.movies.is_up(pending.kind) {
                self.calls.push_back(pending);
                continue;
            }
            let args = pending.args.clone();
            match self
                .movies
                .call(pending.kind, &pending.path, &pending.function, args)
            {
                Ok(()) => {
                    let line = format!(
                        "call: {} {}.{}({})",
                        pending.kind.movie(),
                        pending.path,
                        pending.function,
                        pending
                            .args
                            .iter()
                            .map(|a| format!("{a:?}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    self.say(line);
                }
                Err(_) if pending.tries < RETRY_FRAMES => {
                    pending.tries += 1;
                    self.calls.push_back(pending);
                }
                Err(why) => {
                    if let Some(key) = &pending.cache_key {
                        self.cache.forget(pending.kind, &pending.path, key);
                    }
                    self.say(format!("hud: {} dropped: {why}", pending.function));
                }
            }
        }
    }

    pub(crate) fn frame<S: Strategic>(&mut self, session: &mut S, dt: Duration) {
        self.start_dialog(session);
        self.sync(session);
        self.tutorial_tray = session.tray_allowed();
        self.forward_selection(session);
        self.update_messages();
        self.open_veterancy(session);
        for sent in self.movies.frame(dt) {
            self.clip_event(session, &sent);
        }
        self.advance_prompt();
        self.update_prompt_buttons(session.state());
        self.advance_award(session.state());
        self.pump_dialog_queue();
        self.update_command_ui(session.state());
        self.update_tray(session.state());
        self.update_panels(session.state());
        self.update_retinue(session.state());
        self.update_swapper(session.state());
        self.update_queue(session.state());
        self.update_tray_lock();
        self.pump_calls();
        self.update_end_turn(session.state());
        self.update_checklist(session.state());
        self.update_stats(session.state());
    }

    fn update_stats(&mut self, state: &LivingWorldState) {
        let Some(now) = state.player_stats(self.local) else {
            return;
        };
        let movies = &self.movies;
        let changes = self.stats.changes(now, |key| movies.label(key));
        for (key, text) in changes.texts {
            self.movies.set_text(Kind::Hud, &key, &text);
        }
        if let Some(clip) = changes.flash {
            let flash = vec![Value::str("0")];
            if let Err(why) = self.movies.call(Kind::Hud, &clip, "UpdateStatsText", flash) {
                self.say(format!("hud: stats flash: {why}"));
            }
        }
    }

    fn update_checklist(&mut self, state: &LivingWorldState) {
        let movies = &self.movies;
        let changes = self
            .checklist
            .changes(state.turn, state.phase, |key| movies.label(key));
        for (key, text) in changes {
            self.movies.set_text(Kind::Hud, &key, &text);
        }
    }

    fn forward_selection<S: Strategic>(&mut self, session: &mut S) {
        for signal in self
            .told
            .changes(self.selection.army, self.selection.battle)
        {
            match signal {
                Signal::Deselect(army) => {
                    session.ui(UiAction::DeselectArmy(army));
                }
                Signal::Select(army) => {
                    if !session.ui(UiAction::SelectArmy(army)) {
                        self.told.refused();
                        self.selection.pick_army_in_tray(None);
                        self.say(format!("hud: army {} refused by the tutorial", army.0));
                    }
                }
                Signal::Battle(id) => {
                    let region = session
                        .state()
                        .battles
                        .iter()
                        .find(|b| b.id == id)
                        .map(|b| b.region);
                    if let Some(region) = region {
                        session.ui(UiAction::SelectBattle(region));
                    }
                }
            }
        }
    }

    fn update_messages(&mut self) {
        let Some(message) = self.messages.start_next().cloned() else {
            return;
        };
        let title = self.movies.label(&message.title).unwrap_or(message.title);
        let text = self.movies.label(&message.text).unwrap_or(message.text);
        let text = message::fill(&text, &message.args);
        self.say(format!("hud: message pop-up {title:?} / {text:?}"));
        self.movies.show_message(&title, &text);
    }

    fn close_message(&mut self) {
        if self.messages.close() {
            self.movies.hide_message();
            self.say("hud: message pop-up closed".to_owned());
        }
    }

    pub(crate) fn push_message(&mut self, source: MessageSource, title: &str, text: &str) {
        self.messages.push(Message {
            source,
            title: title.to_owned(),
            text: text.to_owned(),
            args: Vec::new(),
        });
    }

    pub(crate) fn push_territory_message(
        &mut self,
        state: &LivingWorldState,
        territory: usize,
        title: &str,
        text: &str,
    ) {
        let found = state.rules.territories.get(territory);
        let lines: Vec<String> = found
            .map(|t| {
                award::BONUS_LINES
                    .iter()
                    .take(5)
                    .filter_map(|(label, field)| {
                        let value = t.bonus.get(*field);
                        (value != 0)
                            .then(|| self.movies.formatted(label, i64::from(value)))
                            .flatten()
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.messages.push(Message {
            source: MessageSource::Territory,
            title: title.to_owned(),
            text: text.to_owned(),
            args: vec![
                found
                    .map(|t| self.movies.label(&t.name).unwrap_or_else(|| t.name.clone()))
                    .unwrap_or_default(),
                lines.join("\n"),
            ],
        });
    }

    pub(crate) fn tutorial_event(&mut self, event: &TutorialEvent) {
        let line = |hud: &Self, key: &str| hud.movies.label(key).unwrap_or_else(|| key.to_owned());
        match event {
            TutorialEvent::SessionStarted {
                text, clear_text, ..
            } => {
                if *clear_text {
                    self.movies.narration.clear();
                }
                if let Some(key) = text {
                    let shown = line(self, key);
                    self.say(format!("hud: narration {shown:?}"));
                    self.movies.narration.show(&shown, NARRATION_SECONDS);
                }
            }
            TutorialEvent::TaskBegan { task, text } => {
                if matches!(task, Task::EnableAutoResolve(_)) {
                    self.movies.narration.set_area(narration::DIALOG_AREA);
                }
                if let Some(key) = text {
                    let shown = line(self, key);
                    self.say(format!("hud: task hint {shown:?}"));
                    self.movies.narration.show(&shown, NARRATION_SECONDS);
                }
            }
            TutorialEvent::SessionDone { .. } => {
                self.movies.narration.set_area(narration::DEFAULT_AREA);
            }
            TutorialEvent::CompletionPopup { title, text } => {
                self.push_message(MessageSource::Tutorial, title, text);
            }
            _ => {}
        }
    }

    pub(crate) fn message_pending(&self) -> bool {
        self.messages.current().is_some() || self.movies.is_up(Kind::Message)
    }

    pub(crate) fn take_closed_messages(&mut self) -> Vec<MessageSource> {
        self.messages.take_closed()
    }

    fn pump_dialog_queue(&mut self) {
        loop {
            let Some((call, tries)) = self.dialog.as_mut().and_then(|d| d.queue.pop_front()) else {
                break;
            };
            match self.apply(&call) {
                Ok(()) => {}
                Err(_) if tries < RETRY_FRAMES => {
                    if let Some(run) = self.dialog.as_mut() {
                        run.queue.push_front((call, tries.saturating_add(1)));
                    }
                    break;
                }
                Err(why) => self.say(format!("dialog: {call:?} dropped: {why}")),
            }
        }
    }

    fn translated(&self, sent: &Sent) -> ClipEvent {
        if sent.kind == Kind::Hud && sent.event.suffix == "_OnClicked" {
            let element = self
                .frames
                .index_of(&sent.event.clip)
                .and_then(|index| self.attached.get(&index));
            if let Some(command_ui::Button::Element(element)) = element {
                let mut event = sent.event.clone();
                (*element).clone_into(&mut event.clip);
                return event;
            }
        }
        sent.event.clone()
    }

    fn slot_state_call(&mut self, kind: Kind, path: &str, function: &str, slot: u32, label: &str) {
        let key = format!("{function}#{slot}");
        if self.cache.changed(kind, path, &key, label) {
            self.calls.push_back(PendingCall {
                kind,
                path: path.to_owned(),
                function: function.to_owned(),
                args: vec![Value::Number(f64::from(slot)), Value::str(label)],
                tries: 0,
                cache_key: Some(key),
            });
        }
    }

    fn face_image(icon: Option<String>) -> ImageSource {
        icon.map_or(ImageSource::Hidden, ImageSource::MappedImage)
    }
    fn open_prompt(&mut self, state: &LivingWorldState, battle: &Battle) {
        let pages = prompt::pages(state, battle, self.local);
        let teams = mint_sage_livingworld::battle_sides(state, battle);
        let position = teams.iter().enumerate().find_map(|(side, team)| {
            let slot = team.iter().position(|p| *p == self.local)?;
            Some((u8::try_from(side).ok()?, u8::try_from(slot).ok()?))
        });
        self.selection.page = position.unwrap_or((0, 0));
        if let Some(region) = state.rules.region_rules(battle.region) {
            self.movies
                .set_text(Kind::Prompt, "_level0_RegionName", &region.name);
        }
        let heroes = position.map_or(0, |at| {
            crate::wotr_hud::page_heroes(state, battle.id, at).len()
        });
        let names = |side: Side| -> Vec<String> {
            pages
                .iter()
                .filter(|p| p.side == side)
                .map(|p| self.config.face(p.player).name)
                .collect()
        };
        let (ally, enemy) = (
            Tabs::new(Side::Ally, names(Side::Ally)),
            Tabs::new(Side::Enemy, names(Side::Enemy)),
        );
        self.say(format!(
            "hud: battle prompt opens for battle {} with {} page(s)",
            battle.id,
            pages.len()
        ));
        self.prompt = Some(PromptRun {
            battle: battle.id,
            pages,
            heroes,
            added: false,
            tabs: [0, 0],
            ally,
            enemy,
        });
    }

    fn update_prompt_buttons(&mut self, state: &LivingWorldState) {
        let Some(run) = self.prompt.as_ref().filter(|run| run.added) else {
            return;
        };
        let Some(battle) = state.battles.iter().find(|b| b.id == run.battle) else {
            return;
        };
        let mask = state.battle_buttons(
            battle,
            self.local,
            self.config.battle_type(),
            prompt::LOCAL_GAME,
        );
        for (button, label) in prompt::button_states(mask) {
            self.named_state_call(Kind::Prompt, "", "SetButtonState", button, label);
        }
    }

    fn named_state_call(
        &mut self,
        kind: Kind,
        path: &str,
        function: &str,
        name: &str,
        label: &str,
    ) {
        let key = format!("{function}#{name}");
        if self.cache.changed(kind, path, &key, label) {
            self.calls.push_back(PendingCall {
                kind,
                path: path.to_owned(),
                function: function.to_owned(),
                args: vec![Value::str(name), Value::str(label)],
                tries: 0,
                cache_key: Some(key),
            });
        }
    }

    fn advance_prompt(&mut self) {
        let Some(mut run) = self.prompt.take() else {
            return;
        };
        if !run.added {
            if self.movies.exists(Kind::Prompt, "main.allies") {
                self.queue_pages(&run);
                run.added = true;
            }
        } else {
            for (i, side) in [Side::Ally, Side::Enemy].into_iter().enumerate() {
                let members: Vec<prompt::Page> = run
                    .pages
                    .iter()
                    .filter(|p| p.side == side)
                    .copied()
                    .collect();
                if members.is_empty() {
                    continue;
                }
                let clip = Self::tabs_clip(side);
                match run.tabs.get(i).copied().unwrap_or(2) {
                    0 => {
                        self.call(
                            Kind::Prompt,
                            clip,
                            "SetTabCount",
                            vec![Value::Number(members.len().min(5) as f64)],
                        );
                        if let Some(stage) = run.tabs.get_mut(i) {
                            *stage = 1;
                        }
                    }
                    1 if self.movies.exists(Kind::Prompt, &format!("{clip}.0")) => {
                        for (n, page) in members.iter().take(5).enumerate() {
                            let face = self.config.face(page.player);
                            self.movies.set_image(
                                Kind::Prompt,
                                &format!("{}_Image{n}", printed(clip)),
                                Self::face_image(face.faction_icon),
                            );
                            self.call(
                                Kind::Prompt,
                                clip,
                                "SetTabColor",
                                vec![
                                    Value::Number(n as f64),
                                    Value::Number(f64::from(face.color)),
                                ],
                            );
                        }
                        let tabs = match side {
                            Side::Ally => &mut run.ally,
                            Side::Enemy => &mut run.enemy,
                        };
                        let calls = tabs.select(0);
                        self.apply_tab_calls(side, calls);
                        if let Some(stage) = run.tabs.get_mut(i) {
                            *stage = 2;
                        }
                    }
                    _ => {}
                }
            }
        }
        self.prompt = Some(run);
    }

    fn queue_pages(&mut self, run: &PromptRun) {
        for page in &run.pages {
            self.call(
                Kind::Prompt,
                "",
                page.side.add(),
                vec![Value::Number(f64::from(page.id))],
            );
        }
        for side in [Side::Ally, Side::Enemy] {
            if run.pages.iter().any(|p| p.side == side) {
                self.call(Kind::Prompt, "", side.fade_in(), vec![Value::Number(0.0)]);
            }
        }
        for page in &run.pages {
            let face = self.config.face(page.player);
            let path = page.path();
            self.state_call(Kind::Prompt, &path, "SetState", page.state());
            self.call(
                Kind::Prompt,
                &path,
                "SetPlayerColor",
                vec![Value::Number(f64::from(face.color))],
            );
            if page.local {
                for n in 0..3u32 {
                    let (shown, label) = prompt::swap_button(n, run.heroes);
                    self.call(
                        Kind::Prompt,
                        &path,
                        "SetSwapButtonVisibility",
                        vec![Value::Number(f64::from(n)), Value::Bool(shown)],
                    );
                    self.slot_state_call(Kind::Prompt, &path, "SetSwapButtonState", n, label);
                }
            }
        }
    }

    fn tabs_clip(side: Side) -> &'static str {
        match side {
            Side::Ally => "main.allyTabs",
            Side::Enemy => "main.enemyTabs",
        }
    }

    fn apply_tab_calls(&mut self, side: Side, calls: Vec<prompt::TabCall>) {
        let clip = Self::tabs_clip(side);
        for call in calls {
            match call {
                prompt::TabCall::State(tab, label) => self.call(
                    Kind::Prompt,
                    clip,
                    "SetTabState",
                    vec![Value::Number(tab as f64), Value::str(label)],
                ),
                prompt::TabCall::PlayerName(name) => {
                    self.movies.set_text(
                        Kind::Prompt,
                        &format!("{}_PlayerName", printed(clip)),
                        &name,
                    );
                }
                prompt::TabCall::LocalPlayerName(name) => {
                    self.movies.set_text(
                        Kind::Prompt,
                        &format!("{}_LocalPlayerName", printed(clip)),
                        &name,
                    );
                }
                prompt::TabCall::NameVisibility(shown) => self.call(
                    Kind::Prompt,
                    clip,
                    "SetPlayerNameVisibility",
                    vec![Value::Bool(shown)],
                ),
            }
        }
    }

    fn tab_clicked(&mut self, event: &ClipEvent) {
        let lower = event.clip.to_ascii_lowercase();
        let side = if lower.contains("allytabs") {
            Side::Ally
        } else if lower.contains("enemytabs") {
            Side::Enemy
        } else {
            return;
        };
        let Some(tab) = event.index() else { return };
        let Some(run) = self.prompt.as_mut() else {
            return;
        };
        let tabs = match side {
            Side::Ally => &mut run.ally,
            Side::Enemy => &mut run.enemy,
        };
        let old = tabs.selected();
        let calls = tabs.select(tab);
        if calls.is_empty() {
            return;
        }
        if let Some(old) = old.filter(|old| *old != tab) {
            self.call(
                Kind::Prompt,
                "",
                side.fade_out(),
                vec![Value::Number(old as f64)],
            );
        }
        self.call(
            Kind::Prompt,
            "",
            side.fade_in(),
            vec![Value::Number(tab as f64)],
        );
        self.apply_tab_calls(side, calls);
    }

    fn fill_award(&mut self, state: &LivingWorldState, region: RegionId, players: &[PlayerId]) {
        let rules = state.rules.region_rules(region);
        if let Some(rules) = rules {
            self.movies
                .set_text(Kind::Award, "_level0_RegionName", &rules.name);
            for i in 0..award::BONUS_LINES.len() {
                if let Some((label, value)) = award::bonus_line(&rules.bonus, i) {
                    if let Some(text) = self.movies.formatted(label, i64::from(value)) {
                        self.movies.set_text(
                            Kind::Award,
                            &format!("_level0_RegionBonus{i}"),
                            &text,
                        );
                    }
                }
            }
        }
        for (i, row) in award::rows(state, players).into_iter().enumerate() {
            let face = self.config.face(row.player);
            self.movies
                .set_text(Kind::Award, &format!("_level0_PlayerName{i}"), &face.name);
            self.movies.set_text(
                Kind::Award,
                &format!("_level0_NumRegions{i}"),
                &row.regions.to_string(),
            );
            self.movies.set_text(
                Kind::Award,
                &format!("_level0_NumUnits{i}"),
                &row.units.to_string(),
            );
            self.movies.set_image(
                Kind::Award,
                &format!("_level0_PlayerIcon{i}"),
                Self::face_image(face.faction_icon),
            );
            self.call(
                Kind::Award,
                "",
                "SetPlayerColor",
                vec![
                    Value::Number(i as f64),
                    Value::Number(f64::from(face.color)),
                ],
            );
        }
        self.selection.award_row = None;
        self.award_row_clicked(0);
    }

    fn advance_award(&mut self, state: &LivingWorldState) {
        let Some(mut run) = self.award.take() else {
            return;
        };
        if !run.counted {
            if self.movies.exists(Kind::Award, "main") {
                self.call(
                    Kind::Award,
                    "",
                    "SetPlayerCount",
                    vec![Value::Number(run.players.len() as f64)],
                );
                run.counted = true;
            }
        } else if !run.filled
            && (run.players.is_empty() || self.movies.exists(Kind::Award, "main.0"))
        {
            self.fill_award(state, run.region, &run.players);
            run.filled = true;
        }
        self.award = Some(run);
    }

    fn award_row_clicked(&mut self, row: usize) {
        for (r, label) in award::select(self.selection.award_row, row) {
            self.call(
                Kind::Award,
                "",
                "SetPlayerRowState",
                vec![Value::Number(r as f64), Value::str(label)],
            );
        }
        self.selection.award_row = Some(row);
    }

    fn update_command_ui(&mut self, state: &LivingWorldState) {
        let wanted = command_ui::wanted(state, &self.selection, self.local);
        for index in 0..command_ui::FRAMES {
            let (Some(frame), Some(printed_frame)) = (
                self.frames.path(index),
                self.frames.printed(index).map(str::to_owned),
            ) else {
                continue;
            };
            let want = wanted.iter().find(|w| w.frame == index);
            let have = self.attached.get(&index).copied();
            let button = format!("{frame}.{}", command_ui::INSTANCE);
            if want.map(|w| w.button) != have {
                if have.is_some() {
                    self.call(Kind::Hud, &frame, "DeleteContent", Vec::new());
                    self.cache.forget(Kind::Hud, &button, "Enable");
                    self.attached.remove(&index);
                }
                if let Some(want) = want {
                    self.call(
                        Kind::Hud,
                        &frame,
                        "CreateContent",
                        vec![
                            Value::str(command_ui::CONTENT),
                            Value::str(command_ui::INSTANCE),
                        ],
                    );
                    let image = match want.button {
                        command_ui::Button::Element(element) => {
                            self.config.button_image(element).map(str::to_owned)
                        }
                        command_ui::Button::Build(building) => state
                            .rules
                            .building(building)
                            .and_then(|b| self.config.building_image(&b.name))
                            .map(str::to_owned),
                    };
                    self.movies.set_image(
                        Kind::Hud,
                        &format!("{printed_frame}.{}_Image", command_ui::INSTANCE),
                        Self::face_image(image),
                    );
                    self.attached.insert(index, want.button);
                }
            }
            if let Some(want) = want {
                let label = if want.enabled { "true" } else { "false" };
                if self.cache.changed(Kind::Hud, &button, "Enable", label) {
                    self.calls.push_back(PendingCall {
                        kind: Kind::Hud,
                        path: button,
                        function: "Enable".to_owned(),
                        args: vec![Value::Bool(want.enabled)],
                        tries: 0,
                        cache_key: Some("Enable".to_owned()),
                    });
                }
            }
        }
    }

    fn publish(&mut self, page: &str, binds: Vec<panels::Bind>) {
        self.publish_to(Kind::Hud, page, binds);
    }

    fn publish_to(&mut self, kind: Kind, page: &str, binds: Vec<panels::Bind>) {
        let key = printed(page);
        for bind in binds {
            match bind {
                panels::Bind::Call {
                    function,
                    slot,
                    args,
                } => {
                    let cache = format!("{function}#{}", slot.unwrap_or(0));
                    let label = format!("{args:?}");
                    self.published
                        .insert(format!("{key}.{cache}"), label.clone());
                    if self.cache.changed(kind, page, &cache, &label) {
                        self.calls.push_back(PendingCall {
                            kind,
                            path: page.to_owned(),
                            function: function.to_owned(),
                            args,
                            tries: 0,
                            cache_key: Some(cache),
                        });
                    }
                }
                panels::Bind::Text { name, text } => {
                    let text = match text {
                        panels::Text::Plain(text) => text,
                        panels::Text::Formatted(label, value) => {
                            self.movies.formatted(label, value).unwrap_or_default()
                        }
                    };
                    self.published
                        .insert(format!("{key}_{name}"), format!("{text:?}"));
                    self.movies.set_text(kind, &format!("{key}_{name}"), &text);
                }
                panels::Bind::Picture { name, element } => {
                    self.published
                        .insert(format!("{key}_{name}"), format!("{element:?}"));
                    let image = element.and_then(|e| self.config.type_image(&e).map(str::to_owned));
                    self.movies
                        .set_image(kind, &format!("{key}_{name}"), Self::face_image(image));
                }
            }
        }
    }

    fn tray_view(&self, state: &LivingWorldState) -> panels::TrayView {
        let sel = &self.selection;
        let building = sel.region.zip(sel.plot).and_then(|(region, plot)| {
            state.region(region)?.plots.get(usize::from(plot))?.as_ref()
        });
        panels::TrayView {
            region: sel.region.is_some(),
            army_details: sel.plot.is_none()
                && sel.details
                && sel.army.is_some_and(|id| state.armies.contains_key(&id)),
            production_building: building
                .and_then(|b| state.rules.building(b.building))
                .is_some_and(|b| b.queue.is_some()),
        }
    }

    fn update_tray(&mut self, state: &LivingWorldState) {
        let wanted = panels::tray_after(self.tray, self.tray_view(state));
        if wanted == self.tray {
            return;
        }
        if let Some(panel) = wanted {
            self.call(
                Kind::Hud,
                panels::TRAY,
                "loadMovie",
                vec![Value::str(panel.movie())],
            );
            self.cache.clear_under(Kind::Hud, panels::TRAY);
            self.retinue = retinue::RetinueView::at(panels::TRAY);
        }
        self.tray = wanted;
    }

    fn update_tray_lock(&mut self) {
        for step in self.tray_lock.update(self.tutorial_tray, self.tray_open) {
            match step {
                panels::LockStep::Close => {
                    self.call(Kind::Hud, panels::TRAY_SHELL, "Close", Vec::new());
                }
                panels::LockStep::ToggleButton(label) => {
                    self.state_call(Kind::Hud, panels::TRAY_SHELL, "SetToggleButtonState", label);
                }
                panels::LockStep::Flash { frame, seconds } => {
                    self.say(format!(
                        "hud: command frame {frame} flashes for {seconds} s (the flash itself is not drawn)"
                    ));
                }
            }
        }
    }

    fn update_retinue(&mut self, state: &LivingWorldState) {
        if self.tray != Some(panels::Tray::ArmyRetinue)
            || !self.movies.exists(Kind::Hud, panels::RETINUE_READY)
        {
            return;
        }
        let Some(army) = self.selection.army.and_then(|id| state.armies.get(&id)) else {
            return;
        };
        self.publish(panels::TRAY, panels::retinue_layout(army.is_hero_army()));
        let mut view = std::mem::take(&mut self.retinue);
        let picks = self.selection.clone();
        self.fill_retinue(Kind::Hud, &mut view, army, &picks, state);
        self.retinue = view;
    }

    fn update_queue(&mut self, state: &LivingWorldState) {
        if self.tray != Some(panels::Tray::BuildQueue)
            || !self.movies.exists(Kind::Hud, panels::QUEUE_READY)
        {
            return;
        }
        let Some(plot) =
            self.selection
                .region
                .zip(self.selection.plot)
                .and_then(|(region, plot)| {
                    state.region(region)?.plots.get(usize::from(plot))?.as_ref()
                })
        else {
            return;
        };
        let rules = &state.rules;
        let Some(queue) = rules.building(plot.building).and_then(|b| b.queue.as_ref()) else {
            return;
        };
        let slots: Vec<panels::QueueSlot> = plot
            .queue
            .iter()
            .enumerate()
            .filter_map(|(place, entry)| {
                let entry = queue.entries.get(usize::from(*entry))?;
                let spent = if place == 0 {
                    i32::try_from(plot.queue_turns).unwrap_or(i32::MAX)
                } else {
                    0
                };
                let first = entry.roster.first().map(|(unit, _)| *unit);
                let hero = entry.hero.or(first);
                Some(panels::QueueSlot {
                    name: hero
                        .and_then(|u| rules.unit_names.get(usize::from(u.0)).cloned())
                        .unwrap_or_default(),
                    quantity: entry
                        .roster
                        .iter()
                        .fold(0u32, |sum, (_, n)| sum.saturating_add(*n))
                        .saturating_add(u32::from(entry.hero.is_some())),
                    build_time: entry.build_time,
                    turns_remaining: entry.build_time.saturating_sub(spent).max(0),
                    hero: entry.hero.is_some(),
                    command_points: entry.roster.iter().fold(0i32, |sum, (unit, n)| {
                        let each = rules
                            .units
                            .get(usize::from(unit.0))
                            .map_or(0, |u| u.command_points);
                        sum.saturating_add(each.saturating_mul(i32::try_from(*n).unwrap_or(0)))
                    }),
                })
            })
            .collect();
        self.publish(panels::TRAY, panels::build_queue(&slots));
    }

    pub(crate) fn panels_report(&self) -> String {
        let under = printed(panels::TRAY);
        self.published
            .iter()
            .filter(|(key, _)| key.starts_with(&under))
            .map(|(key, value)| format!("{key} = {value}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    }

    fn update_panels(&mut self, state: &LivingWorldState) {
        let Some(region) = self.selection.region else {
            return;
        };
        if self.tray != Some(panels::Tray::Region) {
            return;
        }
        let rules = &state.rules;
        let armies: Vec<_> = state
            .armies_of(self.local)
            .filter(|a| a.region == region)
            .collect();
        let slots: Vec<panels::ArmySlot> = armies
            .iter()
            .map(|a| panels::ArmySlot {
                name: a
                    .hero
                    .and_then(|h| rules.unit_names.get(usize::from(h.0)).cloned())
                    .unwrap_or_else(|| "Garrison".to_owned()),
                hero: a.is_hero_army(),
                command_points: a.roster.iter().fold(0i32, |sum, m| {
                    let each = rules
                        .units
                        .get(usize::from(m.unit.0))
                        .map_or(0, |u| u.command_points);
                    sum.saturating_add(each.saturating_mul(i32::try_from(m.count).unwrap_or(0)))
                }),
            })
            .collect();
        let picked = self
            .selection
            .army
            .and_then(|id| armies.iter().position(|a| a.id == id));
        self.publish(panels::ARMIES, panels::armies(&slots, picked));
        let plots: Vec<panels::PlotSlot> = state
            .region(region)
            .map(|r| r.plots.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|plot| {
                let info = plot.as_ref().and_then(|p| rules.building(p.building));
                panels::PlotSlot {
                    building: info.map(|b| b.name.clone()),
                    kind: info.and_then(|b| b.kind),
                    turns_left: plot.as_ref().map_or(0, |p| p.turns_left),
                    under_construction: plot.as_ref().is_some_and(|p| p.under_construction),
                }
            })
            .collect();
        self.publish(
            panels::STRUCTURES,
            panels::structures(&plots, self.selection.plot.map(usize::from)),
        );
        if let Some(region_rules) = rules.region_rules(region) {
            let mut lines: Vec<(&'static str, i64)> = award::BONUS_LINES
                .iter()
                .take(5)
                .filter_map(|(label, field)| {
                    let value = region_rules.bonus.get(*field);
                    (value != 0).then_some((*label, i64::from(value)))
                })
                .collect();
            if lines.is_empty() {
                lines.push(("STRATEGICHUD:RegionBonusNone", 0));
            }
            self.publish(
                panels::TERRITORY,
                panels::territory(&region_rules.name, &lines),
            );
        }
    }

    fn update_end_turn(&mut self, state: &LivingWorldState) {
        let dialog_open = self.dialog.is_some() || self.movies.is_up(Kind::Prompt);
        let can_end = state.can_end_phase(self.local);
        for action in self.end_turn.update(state.phase, can_end, dialog_open) {
            match action {
                end_turn::Action::Enable(on) => self.state_call(
                    Kind::Hud,
                    end_turn::CLIP,
                    "SetState",
                    end_turn::state_label(on),
                ),
                end_turn::Action::PlayAlertFlash => {
                    self.call(Kind::Hud, end_turn::CLIP, "PlayAlertFlash", Vec::new());
                }
                end_turn::Action::HaltAlertFlash => {
                    self.call(Kind::Hud, end_turn::CLIP, "HaltAlertFlash", Vec::new());
                }
            }
        }
    }

    pub(crate) fn close_dialog(&mut self) {
        if let Some(run) = self.dialog.take() {
            self.movies.show(Kind::Dialog, false);
            self.veterancy_due = Some(run.region);
        }
    }

    pub(crate) fn dialog_report(&mut self) -> String {
        let Some(run) = self.dialog.as_ref() else {
            return "no dialog".to_owned();
        };
        let rows: Vec<(String, String, f32, bool)> = run
            .playback
            .plan()
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                (
                    e.slot.path(),
                    format!("player {}", e.player.0),
                    run.playback.percent_of(i),
                    e.slot.panel == crate::wotr_dialog::Panel::Allies,
                )
            })
            .collect();
        let finished = run.playback.finished();
        let state = run.state;
        let mut out = format!("dialog finished {finished} state {state:?}");
        for (path, who, expected, _) in rows {
            let read = self
                .movies
                .read(Kind::Dialog, &format!("{path}.Bar.Main.Mask"), "_width")
                .map(|w| w / 2.56);
            out.push_str(&format!(
                "\n  {path} {who}: bar read from the movie {} expects {expected:.1}%",
                read.map_or("unread".to_owned(), |b| format!("{b:.1}%")),
            ));
        }
        out
    }

    pub(crate) fn names_report(&self, state: &LivingWorldState) -> String {
        state
            .players
            .iter()
            .map(|p| {
                format!(
                    "player {}: {:?} ({:?}, {:?})",
                    p.id.0,
                    self.config.face(p.id).name,
                    p.kind,
                    p.skill
                )
            })
            .collect::<Vec<_>>()
            .join("\n  ")
    }

    pub(crate) fn frames_report(&self, state: &LivingWorldState) -> String {
        let mut out = format!(
            "tray {:?}; command frames of army {:?} (details {}) plot {:?} members {:?}",
            self.tray.map(panels::Tray::movie),
            self.selection.army.map(|a| a.0),
            self.selection.details,
            self.selection.plot,
            self.selection.members
        );
        for want in command_ui::wanted(state, &self.selection, self.local) {
            let name = match want.button {
                command_ui::Button::Element(element) => element.to_owned(),
                command_ui::Button::Build(id) => state
                    .rules
                    .building(id)
                    .map_or_else(|| format!("building {}", id.0), |b| b.name.clone()),
            };
            let attached = self.attached.get(&want.frame) == Some(&want.button);
            out.push_str(&format!(
                "\n  frame {}: {name} enabled {} attached {attached}",
                want.frame, want.enabled
            ));
        }
        out
    }

    fn toggle_details(&mut self) {
        if self.tutorial_tray == Some(false) {
            self.say("hud: the details tray is not allowed yet".to_owned());
            return;
        }
        let function = if self.tray_open { "Close" } else { "Open" };
        self.call(Kind::Hud, panels::TRAY_SHELL, function, Vec::new());
    }

    fn pick_plot<S: Strategic>(&mut self, session: &mut S, slot: usize) {
        let Ok(plot) = u8::try_from(slot) else {
            self.selection.pick_plot(None);
            return;
        };
        let region = self.selection.region;
        let built = region
            .and_then(|r| session.state().region(r))
            .and_then(|r| r.plots.get(slot))
            .is_some_and(Option::is_some);
        let allowed = region.is_none_or(|region| {
            session.ui(if built {
                UiAction::ClickBuilding { region, plot }
            } else {
                UiAction::ClickPlot { region, plot }
            })
        });
        if allowed {
            self.selection.pick_plot(Some(plot));
            self.say(format!("hud: plot {slot} picked"));
        } else {
            self.say(format!("hud: plot {slot} refused by the tutorial"));
        }
    }

    fn pick_army<S: Strategic>(&mut self, session: &S, slot: usize) {
        let region = self.selection.region;
        let army = session
            .state()
            .armies_of(self.local)
            .filter(|a| Some(a.region) == region)
            .nth(slot)
            .map(|a| a.id);
        self.selection.pick_army_in_tray(army);
        self.say(format!("hud: army slot {slot} -> {:?}", army.map(|a| a.0)));
    }

    pub(crate) fn region_clicked(&mut self, region: RegionId) {
        let picked_before = self.selection.region == Some(region);
        self.selection.region = Some(region);
        self.selection.plot = None;
        if !picked_before {
            self.selection.pick_army_in_tray(None);
            self.say(format!("hud: region {} picked", region.0));
        }
    }

    pub(crate) fn region_ordered<S: Strategic>(&mut self, session: &mut S, region: RegionId) {
        if let Some((army, garrison)) = self.garrison_merge(session.state(), region) {
            self.merge_into(session, army, garrison);
            return;
        }
        let state = session.state();
        let phase = state.phase;
        let Some(army) = self.selection.army else {
            return;
        };
        if state.armies.get(&army).is_none_or(|a| a.region == region) {
            return;
        }
        let command = match phase {
            Phase::PlanRetreats => Command::ChooseRetreat { army, dest: region },
            _ => Command::MoveArmy { army, dest: region },
        };
        let result = session.submit(self.local, command.clone());
        let line = describe(self.local, &command, &result);
        self.say(line);
    }

    fn garrison_merge(
        &self,
        state: &LivingWorldState,
        region: RegionId,
    ) -> Option<(ArmyId, ArmyId)> {
        let army = self.selection.army?;
        if !self.selection.details || state.phase != Phase::Planning {
            return None;
        }
        garrison_target(state, army, region).map(|garrison| (army, garrison))
    }

    pub(crate) fn army_ordered<S: Strategic>(&mut self, session: &mut S, target: ArmyId) -> bool {
        let state = session.state();
        let Some(army) = self.selection.army.filter(|a| *a != target) else {
            return false;
        };
        let own = |id: ArmyId| {
            state
                .armies
                .get(&id)
                .is_some_and(|a| a.player == self.local)
        };
        if !self.selection.details || state.phase != Phase::Planning || !own(army) || !own(target) {
            return false;
        }
        self.merge_into(session, army, target);
        true
    }

    fn merge_into<S: Strategic>(&mut self, session: &mut S, army: ArmyId, target: ArmyId) {
        let picks = self.selection.members.clone();
        match session
            .state()
            .check_swap(self.local, army, target, &picks, &[])
        {
            Err(Rejected::InvalidSwap) => self.open_swapper(army, target),
            Err(why) => self.say(format!(
                "hud: army {} -> {}: no merge ({why:?})",
                army.0, target.0
            )),
            Ok(()) if self.ctrl => self.open_swapper(army, target),
            Ok(()) => {
                let command = Command::SwapArmyMembers {
                    a: army,
                    b: target,
                    a_members: picks,
                    b_members: Vec::new(),
                };
                let result = session.submit(self.local, command.clone());
                let line = describe(self.local, &command, &result);
                self.say(line);
            }
        }
    }

    pub(crate) fn control<S: Strategic>(
        &mut self,
        session: &mut S,
        word: &str,
        arg: &str,
    ) -> Result<(), String> {
        let intent = intent_named(session.state(), word, arg)?;
        match intent {
            Intent::PickAwardRow(row) => self.selection.award_row = Some(row),
            Intent::PickArmy(slot) => self.pick_army(session, slot),
            Intent::PickPlot(slot) => self.pick_plot(session, slot),
            Intent::DialogSkip => self.dialog_skip(session),
            other => self.run_intent(session, &other),
        }
        Ok(())
    }
}
