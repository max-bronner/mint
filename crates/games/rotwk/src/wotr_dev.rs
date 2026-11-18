use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use mint_app::{App, InputEvent, Key, PointerButton, WindowConfig};
use mint_render::Renderer;
use mint_sage_ini::living_world::{load_living_world, LivingWorldData};
use mint_sage_ini::MemorySources;
use mint_sage_livingworld::harness::{play_with_ai, AiRun};
use mint_sage_livingworld::stand_in::catalog_from_object_ini;
use mint_sage_livingworld::tutorial::{
    next_action, Action, Blocked, Tutorial, TutorialEvent, UiAction,
};
use mint_sage_livingworld::{
    apply_with, BattleAnswer, BattleResult, BattleSpec, Command, Event, LivingWorldState, Phase,
    PlayerId, PlayerKind, PlayerSetup, RegionId, Setup, StrategicAi,
};
use mint_sage_worldmap::{
    MapPick, PadKey, PlayerColors, ScrollControls, ScrollFactors, ScrollKey, WorldMap,
};
use mint_vfs::Vfs;

use crate::in_game_menu::MenuRequest;
use crate::wotr_hud::Refused;
use crate::wotr_movies::Movies;
use crate::wotr_resolve::{Forecast, InstallResolver, ResolvedBattle};
use crate::wotr_screens::{Hud, HudConfig, MessageSource, Strategic};
use crate::wotr_voice::{UpdateClock, WotrVoice};

struct Args {
    installs: Vec<PathBuf>,
    hud: bool,
    drive: Option<String>,
    drive_dt_ms: u64,
    scenario: usize,
    players: usize,
    seed: u64,
    turns: u32,
    screenshot: Option<PathBuf>,
    size: (u32, u32),
    tutorial: bool,
}

const USAGE: &str = "usage: mint-rotwk <install dir> [--install <dir>] --wotr-session <scenario> \
                     [--players N] [--seed N] [--turns N] [--screenshot <file.tga>] [--size WxH] [--hud] [--drive <script|@file>] [--drive-dt MS]";

fn parse(args: &[String]) -> Result<Args, String> {
    let mut out = Args {
        installs: Vec::new(),
        hud: false,
        drive: None,
        drive_dt_ms: 16,
        scenario: 0,
        players: 2,
        seed: 1,
        turns: 0,
        screenshot: None,
        size: (1280, 720),
        tutorial: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{arg} wants a value"))
        };
        let number = |v: String| v.parse::<u64>().map_err(|_| format!("{arg}: not a number"));
        match arg.as_str() {
            "--install" => out.installs.push(PathBuf::from(value()?)),
            "--wotr-session" => out.scenario = usize::try_from(number(value()?)?).unwrap_or(0),
            "--players" => out.players = usize::try_from(number(value()?)?).unwrap_or(2),
            "--seed" => out.seed = number(value()?)?,
            "--turns" => out.turns = u32::try_from(number(value()?)?).unwrap_or(0),
            "--screenshot" => out.screenshot = Some(PathBuf::from(value()?)),
            "--hud" => out.hud = true,
            "--wotr-tutorial" => out.tutorial = true,
            "--drive-dt" => out.drive_dt_ms = number(value()?)?,
            "--drive" => {
                let script = value()?;
                out.drive = Some(match script.strip_prefix('@') {
                    Some(path) => std::fs::read_to_string(path)
                        .map_err(|e| format!("--drive @{path}: {e}"))?,
                    None => script,
                });
            }
            "--size" => {
                let raw = value()?;
                let (w, h) = raw.split_once('x').ok_or("--size wants WxH")?;
                out.size = (
                    w.parse().map_err(|_| "--size wants WxH")?,
                    h.parse().map_err(|_| "--size wants WxH")?,
                );
            }
            other if !other.starts_with("--") => {
                out.installs.insert(0, PathBuf::from(other));
            }
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    if out.installs.is_empty() {
        return Err(USAGE.to_owned());
    }
    Ok(out)
}

const TOGGLE_BUTTON: &str = "main.globe.commandUI.0";

const TOGGLE_WAIT_FRAMES: u32 = 15;

const PLAN_STEPS: u32 = 4096;

const HUMAN_IDLE_FRAMES: u32 = 240;

pub(crate) struct Session {
    pub(crate) data: LivingWorldData,
    pub(crate) state: LivingWorldState,
    ai: StrategicAi,
    log: Vec<mint_sage_livingworld::harness::Logged>,
    resolver: InstallResolver,
    forecast: Forecast,
    mapped_images: Arc<mint_sage_ini::MappedImageIndex>,
    gained: Vec<(PlayerId, u16)>,
    tutorial: Option<Tutorial>,
    updates: UpdateClock,
    pub(crate) scripted: bool,
    local_side: String,
    colours: Vec<usize>,
    real_time: Option<BattleSpec>,
    charges: Vec<crate::wotr_screens::Charge>,
}

pub(crate) struct Loaded {
    pub(crate) living: LivingWorldData,
    pub(crate) catalog: mint_sage_livingworld::Catalog,
    pub(crate) sources: MemorySources,
}

pub(crate) fn load_data(vfs: &Vfs) -> Loaded {
    load_data_with(vfs, false)
}

pub(crate) fn load_data_with(vfs: &Vfs, tutorial: bool) -> Loaded {
    let mut sources = MemorySources::new();
    let mut object_texts = Vec::new();
    let paths: Vec<String> = vfs
        .paths_under("data/ini/")
        .map(|p| p.as_str().to_owned())
        .collect();
    for path in paths {
        let Ok(bytes) = vfs.read(&path) else { continue };
        if path.contains("/object/") || path.contains("/gamedata") || path.ends_with("/rank.ini") {
            object_texts.push(String::from_utf8_lossy(&bytes).into_owned());
        }
        sources.insert(&path, bytes);
    }
    let load = if tutorial {
        crate::wotr_tutorial::load_data(&mut sources)
    } else {
        load_living_world(&sources)
    };
    println!("wotr-map: Living World load errors {}", load.errors.len());
    let catalog = catalog_from_object_ini(object_texts.iter().map(String::as_str), &load.data);
    Loaded {
        living: load.data,
        catalog,
        sources,
    }
}

pub(crate) fn playable_templates(data: &LivingWorldData) -> Vec<String> {
    data.player_templates
        .iter()
        .filter(|t| t.faction.is_some() && t.starting_world_cp.is_some())
        .map(|t| t.name.clone())
        .collect()
}

impl Session {
    fn start_tutorial(vfs: &Vfs, args: &Args) -> Result<Self, String> {
        Self::tutorial(&load_data_with(vfs, true), args.seed)
    }

    pub(crate) fn tutorial(loaded: &Loaded, seed: u64) -> Result<Self, String> {
        let campaign = loaded
            .living
            .campaigns
            .iter()
            .find(|c| !c.tutorials.is_empty())
            .map(|c| c.name.clone())
            .ok_or("no campaign with a Tutorial block; mount an install that includes wotrtutorial.inc")?;
        let setup =
            mint_sage_livingworld::tutorial::scripted_setup(&loaded.living, &campaign, seed)
                .map_err(|e| e.to_string())?;
        let mut session = Self::from_setup(loaded, &setup)?;
        let tutorial = Tutorial::new(&session.data, &session.state).map_err(|e| e.to_string())?;
        session.tutorial = Some(tutorial);
        Ok(session)
    }

    fn start(vfs: &Vfs, args: &Args) -> Result<Self, String> {
        let loaded = load_data(vfs);
        let data = &loaded.living;
        let campaign = data
            .campaigns
            .iter()
            .filter(|c| !c.scenarios.is_empty())
            .nth(args.scenario)
            .map(|c| c.name.clone())
            .ok_or_else(|| format!("no campaign with a scenario at index {}", args.scenario))?;
        let templates = playable_templates(data);
        let players = (0..args.players)
            .map(|i| PlayerSetup {
                template: templates
                    .get(i % templates.len().max(1))
                    .cloned()
                    .unwrap_or_default(),
                kind: if args.hud && i == 0 {
                    PlayerKind::Human
                } else {
                    PlayerKind::Computer
                },
                team: i32::try_from(i).unwrap_or(0),
                start_region: None,
                skill: mint_sage_livingworld::Skill::Medium,
            })
            .collect();
        let mut setup = Setup {
            campaign: campaign.clone(),
            scenario: 0,
            victory_type: 0,
            battle_type: mint_sage_livingworld::screen_queries::BattleType::default(),
            seed: args.seed,
            players,
        };
        crate::wotr_setup::give_start_regions(data, &loaded.catalog, &mut setup)
            .map_err(|e| format!("setup refused: {e}"))?;
        Self::from_setup(&loaded, &setup)
    }

    pub(crate) fn from_setup(loaded: &Loaded, setup: &Setup) -> Result<Self, String> {
        let data = loaded.living.clone();
        let state = LivingWorldState::start(&data, &loaded.catalog, setup)
            .map_err(|e| format!("setup refused: {e}"))?;
        println!(
            "wotr-map: campaign {} players {} seed {} opening 0x{:016x}",
            setup.campaign,
            setup.players.len(),
            setup.seed,
            state.state_hash()
        );
        let ai = StrategicAi::for_slots(&state, false);
        Ok(Self::around(loaded, state, ai, local_side(setup)))
    }

    pub(crate) fn resume(
        loaded: &Loaded,
        state: LivingWorldState,
        ai: Option<StrategicAi>,
    ) -> Self {
        let ai = ai.unwrap_or_else(|| StrategicAi::for_slots(&state, false));
        let side = state
            .players
            .iter()
            .find(|p| p.kind == PlayerKind::Human)
            .and_then(|p| state.rules.templates.get(usize::from(p.template.0)))
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let colours = state.seat_colours.iter().map(|c| *c as usize).collect();
        Self::around(loaded, state, ai, side).with_colours(colours)
    }

    fn around(
        loaded: &Loaded,
        state: LivingWorldState,
        ai: StrategicAi,
        local_side: String,
    ) -> Self {
        let data = loaded.living.clone();
        let mapped_images = Arc::new(mapped_images(&loaded.sources));
        let resolver = InstallResolver::new(&data, &loaded.sources, &state.rules.unit_names);
        println!(
            "wotr-map: auto-resolve tables hold {} template(s)",
            resolver.template_count()
        );
        Self {
            data,
            state,
            ai,
            log: Vec::new(),
            forecast: resolver.forecast(),
            resolver,
            mapped_images,
            gained: Vec::new(),
            tutorial: None,
            updates: UpdateClock::default(),
            scripted: false,
            local_side,
            colours: Vec::new(),
            real_time: None,
            charges: Vec::new(),
        }
    }

    pub(crate) fn with_colours(mut self, colours: Vec<usize>) -> Self {
        self.state.seat_colours = colours
            .iter()
            .map(|c| u32::try_from(*c).unwrap_or(u32::MAX))
            .collect();
        self.colours = colours;
        self
    }

    pub(crate) fn save(&self, stem: &str, description: &str) -> std::io::Result<PathBuf> {
        if self.tutorial.is_some() {
            return Err(std::io::Error::other("the tutorial is not saved"));
        }
        crate::wotr_save::write_session(&self.state, Some(&self.ai), stem, description)
    }

    pub(crate) fn saveable(&self) -> bool {
        self.tutorial.is_none() && self.state.check_saveable().is_ok()
    }

    fn colours_report(&self, vfs: &Vfs) -> String {
        let offered = offered_colours(vfs);
        let seats: Vec<String> = (0..self.state.players.len())
            .map(|seat| {
                let position = self.colours.get(seat).copied().unwrap_or(seat);
                let name = offered
                    .get(position % offered.len().max(1))
                    .map_or("?", |c| c.name.as_str());
                format!("seat {seat} {name} (list position {position})")
            })
            .collect();
        format!("seat colours: {}", seats.join(", "))
    }

    pub(crate) fn tutorial_running(&self) -> bool {
        self.tutorial.is_some()
    }

    pub(crate) fn advance_tutorial(
        &mut self,
        dt_ms: f64,
        voice: &mut WotrVoice,
    ) -> Vec<TutorialEvent> {
        voice.advance(dt_ms);
        let mut out = Vec::new();
        for _ in 0..self.updates.due(dt_ms) {
            let Some(tutorial) = self.tutorial.as_mut() else {
                break;
            };
            let tick = tutorial.tick(&mut self.state, Some(&mut self.resolver), voice);
            out.extend(tick.tutorial);
            self.note(tick.sim);
        }
        out
    }

    fn scripted_action(&self) -> Option<Action> {
        let tutorial = self.tutorial.as_ref().filter(|_| self.scripted)?;
        next_action(tutorial, &self.state)
    }

    pub(crate) fn tutorial_popup_closed(&mut self) -> Option<TutorialEvent> {
        self.tutorial.as_mut()?.popup_closed()
    }

    pub(crate) fn play_turn(&mut self) {
        if self.human().is_some() {
            println!(
                "wotr-map: a human seat plays through the HUD; n and Space are for all-AI sessions"
            );
            return;
        }
        if self.state.ended.is_some() {
            println!("wotr-map: the session has ended {:?}", self.state.ended);
            return;
        }
        let run = AiRun {
            ai: &mut self.ai,
            predictor: &mut self.forecast,
        };
        let outcome = play_with_ai(&mut self.state, &mut self.resolver, run, 1, &mut self.log);
        self.gained.extend(outcome.gained.iter().copied());
        println!(
            "wotr-map: turn {} rejected {} hash 0x{:016x} regions {:?}",
            self.state.turn,
            outcome.rejected,
            self.state.state_hash(),
            self.state
                .players
                .iter()
                .map(|p| self.state.regions_owned(p.id))
                .collect::<Vec<_>>()
        );
    }

    pub(crate) fn show_on(&mut self, map: &mut WorldMap) {
        for (player, territory) in self.gained.drain(..) {
            map.territory_unified(player, territory);
        }
        map.observe(&self.state);
        map.set_palantir_markers(
            self.state
                .palantir_markers
                .iter()
                .map(|m| m.position)
                .collect(),
        );
    }

    fn plan_orders(&mut self) -> usize {
        let mut applied = 0usize;
        for _ in 0..PLAN_STEPS {
            let Some(commands) = self.ai.step(&mut self.state, &mut self.forecast) else {
                break;
            };
            let ending = commands.iter().any(|(_, c)| matches!(c, Command::EndPhase));
            for (player, command) in commands {
                if !matches!(command, Command::EndPhase) && self.apply_raw(player, command).is_ok()
                {
                    applied = applied.saturating_add(1);
                }
            }
            if ending {
                break;
            }
        }
        applied
    }

    pub(crate) fn human(&self) -> Option<PlayerId> {
        self.state
            .players
            .iter()
            .find(|p| p.kind == PlayerKind::Human)
            .map(|p| p.id)
    }

    fn apply_raw(&mut self, player: PlayerId, command: Command) -> Result<(), Refused> {
        let events = match self.tutorial.as_mut() {
            Some(tutorial) => tutorial
                .apply(
                    &mut self.state,
                    Some(&mut self.resolver),
                    player,
                    command.clone(),
                )
                .map_err(|blocked| match blocked {
                    Blocked::ByTutorial => Refused::Tutorial,
                    Blocked::Rejected(reason) => Refused::Engine(reason),
                })?,
            None => apply_with(
                &mut self.state,
                Some(&mut self.resolver),
                player,
                command.clone(),
            )?,
        };
        self.log.push((player, command));
        self.note(events);
        Ok(())
    }

    fn note(&mut self, events: Vec<Event>) {
        for event in events {
            match event {
                Event::PhaseChanged { turn, phase, hash } => {
                    println!("wotr-session: turn {turn} {phase:?} hash 0x{hash:016x}");
                }
                Event::BattlePending { battle, region } => {
                    println!(
                        "wotr-session: battle {battle} pending at region {}",
                        region.0
                    );
                }
                Event::BattleFinished { region, winner } => {
                    println!(
                        "wotr-session: battle at region {} finished, winning side {winner:?}",
                        region.0
                    );
                }
                Event::BattleSpec(spec) => {
                    println!(
                        "wotr-session: battle {} waits for a real-time result ({} side(s))",
                        spec.battle,
                        spec.sides.len()
                    );
                    self.real_time = Some(spec);
                }
                Event::MemberCharged {
                    army,
                    member,
                    price,
                } => self.charges.push((army, member, price)),
                Event::TerritoryGained { player, territory } => {
                    self.gained.push((player, territory));
                }
                Event::RegionCaptured { region, player } => {
                    println!(
                        "wotr-session: region {} captured by player {}",
                        region.0, player.0
                    );
                }
                _ => {}
            }
        }
    }

    pub(crate) fn take_real_time_battle(&mut self) -> Option<BattleSpec> {
        self.real_time.take()
    }

    pub(crate) fn finish_real_time_battle(
        &mut self,
        spec: &BattleSpec,
        result: BattleResult,
    ) -> Result<(), Refused> {
        let human = self.human();
        let issuer = spec
            .participants
            .iter()
            .map(|p| p.player)
            .find(|p| Some(*p) == human)
            .or_else(|| spec.participants.first().map(|p| p.player))
            .unwrap_or(PlayerId(0));
        let region = spec.region;
        self.submit(issuer, Command::ApplyBattleResult { region, result })
    }

    pub(crate) fn pump_ai(&mut self) {
        for _ in 0..4096 {
            if self.state.ended.is_some()
                || self.state.awaiting.is_some()
                || self.tutorial.is_some()
            {
                return;
            }
            let mut stepped = false;
            let before = (
                self.state.turn,
                self.state.phase,
                self.log.len(),
                self.state
                    .battles
                    .iter()
                    .map(|b| b.choices.len())
                    .sum::<usize>(),
            );
            match self.state.phase {
                Phase::Planning | Phase::PlanRetreats => {
                    if let Some(commands) = self.ai.step(&mut self.state, &mut self.forecast) {
                        stepped = true;
                        for (player, command) in commands {
                            if let Err(reason) = self.apply_raw(player, command.clone()) {
                                println!(
                                    "wotr-session: ai {} {command:?} refused: {reason:?}",
                                    player.0
                                );
                            }
                        }
                    }
                }
                Phase::ResolveBattles => self.ai_battles(),
                _ => {}
            }
            let after = (
                self.state.turn,
                self.state.phase,
                self.log.len(),
                self.state
                    .battles
                    .iter()
                    .map(|b| b.choices.len())
                    .sum::<usize>(),
            );
            if before == after && !stepped {
                return;
            }
        }
    }

    fn ai_battles(&mut self) {
        let battles = self.state.battles.clone();
        for battle in battles {
            for player in battle.players.clone() {
                let asked = self.state.kind(player) == Some(PlayerKind::Computer)
                    && self
                        .state
                        .battles
                        .iter()
                        .find(|b| b.id == battle.id)
                        .is_some_and(|b| !b.choices.iter().any(|(p, _)| *p == player));
                if !asked {
                    continue;
                }
                let method = match self.ai.battle_answer(
                    &mut self.state,
                    &mut self.forecast,
                    player,
                    battle.region,
                ) {
                    BattleAnswer::AutoResolve => 1,
                    BattleAnswer::Withdraw => 2,
                    BattleAnswer::RealTime => 3,
                    BattleAnswer::Undecided | BattleAnswer::Abstain => continue,
                };
                let command = Command::ChooseBattleMode {
                    battle: battle.id,
                    method,
                };
                if let Err(reason) = self.apply_raw(player, command) {
                    println!(
                        "wotr-session: ai {} battle answer refused: {reason:?}",
                        player.0
                    );
                }
            }
        }
        let choice = self
            .state
            .decisions
            .values()
            .find(|d| !d.resolved)
            .and_then(|d| {
                let chooser = self.state.decision_chooser(d)?;
                (self.state.kind(chooser) == Some(PlayerKind::Computer))
                    .then(|| (chooser, d.region, d.players.first().copied()))
            });
        if let Some((chooser, region, Some(player))) = choice {
            let _ = self.apply_raw(chooser, Command::ChooseSharedCapture { region, player });
        }
    }
}

impl Strategic for Session {
    fn state(&self) -> &LivingWorldState {
        &self.state
    }

    fn submit(&mut self, player: PlayerId, command: Command) -> Result<(), Refused> {
        self.apply_raw(player, command)?;
        self.pump_ai();
        Ok(())
    }

    fn ui(&mut self, action: UiAction) -> bool {
        match self.tutorial.as_mut() {
            Some(tutorial) => tutorial.ui(&self.state, action),
            None => true,
        }
    }

    fn battle_selectable(&self, region: RegionId) -> bool {
        self.tutorial
            .as_ref()
            .is_none_or(|t| t.battle_selectable(&self.state, region))
    }

    fn tray_allowed(&self) -> Option<bool> {
        self.tutorial.as_ref().map(Tutorial::details_tray_allowed)
    }

    fn resolved(&self) -> Option<&ResolvedBattle> {
        self.resolver.last.as_ref()
    }

    fn take_charges(&mut self) -> Vec<crate::wotr_screens::Charge> {
        std::mem::take(&mut self.charges)
    }
}

fn local_side(setup: &Setup) -> String {
    setup
        .players
        .iter()
        .find(|p| p.kind == PlayerKind::Human)
        .or_else(|| setup.players.first())
        .map(|p| p.template.clone())
        .unwrap_or_default()
}

fn mapped_images(sources: &MemorySources) -> mint_sage_ini::MappedImageIndex {
    let table = mint_sage_ini::BlockTable::corpus_path();
    let reader = mint_sage_ini::Reader::new(sources);
    let mut index = mint_sage_ini::MappedImageIndex::new();
    for path in sources.paths().filter(|p| p.contains("mappedimage")) {
        if let Ok(document) = reader.read(path, &table) {
            index.insert_document(&document);
        }
    }
    index
}

#[derive(Debug, Clone)]
pub(crate) struct OfferedColour {
    pub(crate) name: String,
    pub(crate) rgb: [u8; 3],
    pub(crate) colors: PlayerColors,
}

pub(crate) fn offered_colours(vfs: &Vfs) -> Vec<OfferedColour> {
    let (index, _) = mint_sage_model::load_multiplayer_colors(vfs);
    (0..index.len())
        .filter_map(|i| index.nth(i))
        .filter(|(_, def)| def.available_in_wotr == Some(true))
        .filter_map(|(name, def)| {
            Some(OfferedColour {
                name: name.to_string(),
                rgb: def.rgb.map_or([255; 3], |rgb| [rgb.r, rgb.g, rgb.b]),
                colors: PlayerColors {
                    fill: def.living_world?.to_unit(),
                    banner: def.living_world_banner.or(def.rgb)?.to_unit(),
                },
            })
        })
        .collect()
}

fn player_colors(vfs: &Vfs, session: &Session) -> Vec<PlayerColors> {
    let offered = offered_colours(vfs);
    (0..session.state.players.len())
        .map(|seat| {
            let position = session.colours.get(seat).copied().unwrap_or(seat);
            offered.get(position % offered.len().max(1)).map_or(
                PlayerColors {
                    fill: [1.0; 3],
                    banner: [1.0; 3],
                },
                |c| c.colors,
            )
        })
        .collect()
}

pub(crate) fn seat_swatches(vfs: &Vfs, session: &Session) -> Vec<[u8; 3]> {
    let offered = offered_colours(vfs);
    (0..session.state.players.len())
        .map(|seat| {
            let position = session.colours.get(seat).copied().unwrap_or(seat);
            offered
                .get(position % offered.len().max(1))
                .map_or([255; 3], |c| c.rgb)
        })
        .collect()
}

fn open_map(renderer: &mut Renderer, vfs: &Arc<Vfs>, session: &Session) -> WorldMap {
    let colors = player_colors(vfs, session);
    let (map, report) = WorldMap::load(
        renderer,
        Arc::clone(vfs),
        &session.data,
        &session.state,
        colors,
    );
    for (name, why) in &report.missing {
        println!("wotr-map: model {name} not loaded: {why}");
    }
    let mut map = map;
    map.set_local_player(
        session
            .human()
            .or_else(|| session.state.players.first().map(|p| p.id)),
    );
    let (turbo, factors) = map_settings(vfs);
    map.turbo_enabled = turbo;
    map.controls = ScrollControls::new(factors);
    println!("wotr-map: scroll factors {factors:?}");
    for (name, parts, hidden, matched) in &report.effects {
        println!(
            "wotr-map: effect {name}: {parts} part(s), {hidden} hidden, {matched} key(s) matched"
        );
    }
    println!(
        "wotr-map: {} region(s), {} pick triangle(s), unmatched {:?}, mesh-centre minus CenterPoint {:?}",
        map.layout.regions.len(),
        report.pick_triangles,
        report.unmatched_regions,
        report.center_offset
    );
    println!(
        "wotr-map: start camera focus {:?} distance {:.2} pitch {:.2} deg zoom {}",
        map.camera.focus(),
        map.camera.distance(),
        map.camera.pitch().to_degrees(),
        map.camera.zoom()
    );
    map
}

type Started = Vec<(usize, &'static str, &'static str, Option<String>)>;

fn print_events(map: &mut WorldMap) -> Started {
    let started = map.take_started();
    for event in &started {
        println!(
            "wotr-map: territory event {}: {} / {}, EVA {:?}, fly to {:?} zoom {} heading {}",
            event.territory,
            event.title,
            event.text,
            event.eva,
            event.look_at,
            event.zoom,
            event.heading
        );
    }
    started
        .into_iter()
        .map(|event| (event.territory, event.title, event.text, event.eva))
        .collect()
}

fn parsed(vfs: &Vfs, path: &str) -> Option<mint_sage_ini::Document> {
    let bytes = vfs.read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    mint_sage_ini::Document::parse(path, &text, &mint_sage_ini::BlockTable::corpus_path()).ok()
}

fn map_settings(vfs: &Vfs) -> (bool, ScrollFactors) {
    let game_data = parsed(vfs, "data/ini/gamedata.ini")
        .map(|document| mint_sage_ini::game_data::merged_game_data(&[document]));
    let field = |name: &str| {
        game_data
            .as_ref()
            .and_then(|block| block.field(name))
            .and_then(mint_sage_ini::Field::value)
    };
    let float = |name: &str, default: f32| {
        field(name)
            .and_then(mint_sage_ini::value::parse_float)
            .unwrap_or(default)
    };
    let flag = |name: &str| field(name).and_then(mint_sage_ini::value::parse_bool);
    let move_anchor = parsed(vfs, "data/ini/ingameui.ini")
        .and_then(|document| {
            document
                .roots()
                .iter()
                .filter(|block| block.keyword.eq_ignore_ascii_case("InGameUI"))
                .filter_map(|block| {
                    block
                        .field("MoveRMBScrollAnchor")
                        .and_then(mint_sage_ini::Field::value)
                        .and_then(mint_sage_ini::value::parse_bool)
                })
                .next_back()
        })
        .unwrap_or(false);
    let screen = crate::user_options::UserOptions::load()
        .scroll_factor()
        .unwrap_or_else(|| float("KeyboardDefaultScrollSpeedFactor", 1.0));
    let factors = ScrollFactors {
        horizontal: screen,
        vertical: screen,
        keyboard: float("KeyboardScrollSpeedFactor", 1.0),
        edge: float("ScreenEdgeScrollSpeedFactor", 1.0),
        move_anchor,
        edge_scroll: !flag("Windowed").unwrap_or(false),
        rotate: float("KeyboardCameraRotateSpeed", 1.0),
    };
    (flag("LivingWorldTurbo").unwrap_or(false), factors)
}

fn print_drawn(map: &WorldMap, state: &LivingWorldState) {
    let drawn: Vec<Option<u8>> = map
        .summary()
        .region_owners
        .iter()
        .map(|o| o.map(|p| p.0))
        .collect();
    let held: Vec<Option<u8>> = state
        .regions
        .iter()
        .map(|r| r.owner.filter(|_| r.enabled).map(|p| p.0))
        .collect();
    let armies = state.armies.values().filter(|a| !a.destroy_queued).count();
    println!(
        "wotr-map: turn {} drawn owners == state owners: {} ({} owned); army icons drawn {} of {} armies",
        state.turn,
        drawn == held,
        held.iter().filter(|o| o.is_some()).count(),
        map.summary().army_icons,
        armies
    );
    println!(
        "wotr-map: army lines drawn {}, markers drawn {}",
        map.summary().army_lines,
        map.summary().markers
    );
    println!("wotr-map: drawn owners {drawn:?}");
}

fn screenshots(
    vfs: &Arc<Vfs>,
    mut session: Session,
    args: &Args,
    path: &std::path::Path,
) -> Result<(), String> {
    let mut renderer = crate::open_offscreen_renderer(args.size)?;
    let mut map = open_map(&mut renderer, vfs, &session);
    let stem = path.with_extension("");
    let shoot =
        |map: &mut WorldMap, session: &Session, renderer: &mut Renderer| -> Result<(), String> {
            map.draw(renderer, &session.state)
                .map_err(|e| e.to_string())?;
            print_drawn(map, &session.state);
            let pixels = renderer.read_pixels().map_err(|e| e.to_string())?;
            let (w, h) = renderer.size();
            let out = PathBuf::from(format!("{}-t{}.tga", stem.display(), session.state.turn));
            crate::game::write_tga(&out, w, h, &pixels).map_err(|e| e.to_string())?;
            println!("wotr-map: wrote {}", out.display());
            Ok(())
        };
    shoot(&mut map, &session, &mut renderer)?;
    if args.turns > 0 {
        for _ in 0..args.turns {
            let planned = session.plan_orders();
            session.show_on(&mut map);
            map.advance(1.0e6);
            println!(
                "wotr-map: turn {} planning: {planned} command(s), army lines {}",
                session.state.turn,
                map.army_lines(&session.state).len()
            );
            session.play_turn();
            let raised = session.gained.len();
            session.show_on(&mut map);
            println!(
                "wotr-map: turn {} resolved: territory events raised {raised}, queued for the local player {}",
                session.state.turn,
                map.events_pending()
            );
            map.advance(1.0e6);
            print_events(&mut map);
        }
        session.show_on(&mut map);
        println!("wotr-map: {} army walk(s) started", map.walking_count());
        map.advance(1.0e6);
        print_events(&mut map);
        shoot(&mut map, &session, &mut renderer)?;
        let planned = session.plan_orders();
        session.show_on(&mut map);
        map.advance(1.0e6);
        println!(
            "wotr-map: planning orders applied {planned}, army lines {}",
            map.army_lines(&session.state).len()
        );
        let out = PathBuf::from(format!(
            "{}-t{}-plan.tga",
            stem.display(),
            session.state.turn
        ));
        map.draw(&mut renderer, &session.state)
            .map_err(|e| e.to_string())?;
        println!(
            "wotr-map: planning shot: army lines drawn {}, markers drawn {}",
            map.summary().army_lines,
            map.summary().markers
        );
        let pixels = renderer.read_pixels().map_err(|e| e.to_string())?;
        let (w, h) = renderer.size();
        crate::game::write_tga(&out, w, h, &pixels).map_err(|e| e.to_string())?;
        println!("wotr-map: wrote {}", out.display());
    }
    Ok(())
}

pub(crate) struct WotrApp {
    session: Session,
    map: WorldMap,
    hud: Option<Hud>,
    viewport: (u32, u32),
    pointer: (f32, f32),
    pointer_seen: bool,
    presses: [Option<[f32; 2]>; 2],
    last_hash: u64,
    exit: bool,
    voice: WotrVoice,
    toggle_wait: u32,
    report_in: Option<u32>,
    zoom: Option<ZoomIn>,
}

struct ZoomIn {
    spec: BattleSpec,
    back: ([f32; 2], f32),
    arrived: bool,
}

impl WotrApp {
    pub(crate) fn open(
        renderer: &mut Renderer,
        vfs: &Arc<Vfs>,
        installs: &[PathBuf],
        session: Session,
        silent: bool,
    ) -> Result<Self, String> {
        let mut map = open_map(renderer, vfs, &session);
        println!("wotr-map: {}", session.colours_report(vfs));
        println!("wotr-map: battle type {:?}", session.state.battle_type);
        let hud = open_hud(vfs, installs, &session, renderer)?;
        map.hold_events = hud.is_some();
        let voice = WotrVoice::open(Arc::clone(vfs), session.local_side.clone(), silent);
        println!("wotr-map: overlay {} (m cycles)", map.overlay.label());
        let last_hash = session.state.state_hash();
        let mut app = Self {
            session,
            map,
            hud,
            viewport: (0, 0),
            pointer: (0.0, 0.0),
            pointer_seen: false,
            presses: [None, None],
            last_hash,
            exit: false,
            voice,
            toggle_wait: 0,
            report_in: None,
            zoom: None,
        };
        app.set_viewport(renderer.size());
        Ok(app)
    }

    pub(crate) fn session(&self) -> &Session {
        &self.session
    }

    pub(crate) fn battle_ready(&mut self) -> Option<BattleSpec> {
        if let Some(zoom) = &mut self.zoom {
            if zoom.arrived || self.map.camera.flying() {
                return None;
            }
            zoom.arrived = true;
            return Some(zoom.spec.clone());
        }
        let spec = self.session.take_real_time_battle()?;
        match self.map.zoom_in_on(spec.region) {
            Some(back) => {
                println!(
                    "wotr: zoom-in toward {} ({} frame flight)",
                    spec.region_name,
                    self.map.camera.fly_frames()
                );
                self.zoom = Some(ZoomIn {
                    spec,
                    back,
                    arrived: false,
                });
                None
            }
            None => Some(spec),
        }
    }

    pub(crate) fn finish_real_time_battle(
        &mut self,
        spec: &BattleSpec,
        result: BattleResult,
    ) -> Result<(), Refused> {
        if let Some(zoom) = self.zoom.take() {
            self.map
                .camera
                .fly_to(zoom.back.0, zoom.back.1, self.map.camera.yaw());
        }
        self.session.finish_real_time_battle(spec, result)?;
        self.session.show_on(&mut self.map);
        if let Some(hud) = self.hud.as_mut() {
            if spec.participants.iter().any(|p| p.player == hud.local) {
                hud.veterancy_follows(spec.region);
            }
        }
        Ok(())
    }

    pub(crate) fn play_turn(&mut self) {
        let Some(hud) = self.hud.as_mut() else {
            self.session.play_turn();
            self.session.show_on(&mut self.map);
            return;
        };
        let turn = self.session.state.turn;
        let frame = std::time::Duration::from_millis(16);
        let mut idle = 0u32;
        while idle < HUMAN_IDLE_FRAMES {
            if self.session.state.ended.is_some() || self.session.state.turn != turn {
                break;
            }
            hud.frame(&mut self.session, frame);
            hud.close_veterancy();
            let _ = hud.control(&mut self.session, "skip", "");
            let before = self.session.state.state_hash();
            for step in [
                &[("pick-award", "0"), ("confirm-award", "")][..],
                &[("mode", "auto")],
                &[("endturn", "")],
            ] {
                for (word, arg) in step {
                    let _ = hud.control(&mut self.session, word, arg);
                }
                if self.session.state.state_hash() != before {
                    break;
                }
            }
            if self.session.state.state_hash() == before {
                idle += 1;
            } else {
                idle = 0;
            }
        }
        if idle >= HUMAN_IDLE_FRAMES {
            println!("wotr-map: the human seat cannot advance at turn {turn}");
        }
        self.session.show_on(&mut self.map);
    }

    fn scripted_step(&mut self) {
        let Some(hud) = self.hud.as_mut() else { return };
        if let Some(left) = self.report_in {
            if left == 0 {
                self.report_in = None;
                hud.say(format!(
                    "scripted: panels
  {}",
                    hud.panels_report()
                ));
            } else {
                self.report_in = Some(left - 1);
            }
        }
        if self.toggle_wait > 0 {
            self.toggle_wait -= 1;
            return;
        }
        if hud.message_pending() {
            if let Err(why) = hud.movies.click(
                crate::wotr_movies::Kind::Message,
                crate::wotr_movies::OK_BUTTON_CLIP,
            ) {
                hud.say(format!(
                    "scripted: the pop-up is not ready to click ({why})"
                ));
            }
            return;
        }
        match self.session.scripted_action() {
            Some(Action::Command(command)) => {
                if let Command::QueueArmy { region, plot, .. } = &command {
                    if hud.selection.region != Some(*region) {
                        hud.region_clicked(*region);
                    }
                    if hud.selection.plot != Some(*plot) {
                        let _ = hud.control(&mut self.session, "pick-plot", &plot.to_string());
                    }
                }
                let result = self.session.submit(hud.local, command.clone());
                let line = crate::wotr_hud::describe(hud.local, &command, &result);
                hud.say(format!("scripted: {line}"));
                if matches!(command, Command::QueueArmy { .. }) && result.is_ok() {
                    self.report_in = Some(4);
                }
            }
            Some(Action::Ui(UiAction::SelectArmy(army))) => {
                if hud.selection.pick_army_on_map(Some(army)) {
                    hud.selection.pick_every_member(&self.session.state);
                }
            }
            Some(Action::Ui(UiAction::HudDetailsSignal)) => {
                match hud
                    .movies
                    .press(crate::wotr_movies::Kind::Hud, TOGGLE_BUTTON)
                {
                    Ok(_) => self.toggle_wait = TOGGLE_WAIT_FRAMES,
                    Err(why) => {
                        hud.say(format!("scripted: the details toggle is not ready ({why})"));
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn wants_exit(&self) -> bool {
        self.exit
    }

    pub(crate) fn leave(&mut self) {
        self.voice.stop_map_sounds();
        self.exit = true;
    }

    fn set_viewport(&mut self, viewport: (u32, u32)) {
        self.viewport = viewport;
        let side = |n: u32| f32::from(u16::try_from(n).unwrap_or(u16::MAX));
        self.map
            .controls
            .set_viewport(side(viewport.0), side(viewport.1));
    }

    fn update_hover(&mut self) {
        if !self.pointer_seen {
            return;
        }
        let under = self.map.pick(self.viewport, self.pointer.0, self.pointer.1);
        let (region, army) = match under {
            Some(MapPick::Army { army, region }) => (Some(region), Some(army)),
            Some(MapPick::Region(region)) => (Some(region), None),
            None => (None, None),
        };
        self.map.set_hovered(region);
        self.map.set_hovered_army(army);
    }

    fn click(&mut self, button: usize, pressed: bool) -> Option<[f32; 2]> {
        let at = [self.pointer.0, self.pointer.1];
        let slot = self.presses.get_mut(button)?;
        if pressed {
            *slot = Some(at);
            return None;
        }
        let press = slot.take()?;
        mint_sage_worldmap::click_point(press, at, crate::pointer::DRAG_TOLERANCE_PIXELS)
    }

    fn order_map(&mut self, at: [f32; 2]) {
        let (army, region) = match self.map.pick(self.viewport, at[0], at[1]) {
            Some(MapPick::Army { army, region }) => (Some(army), region),
            Some(MapPick::Region(region)) => (None, region),
            None => return,
        };
        if let Some(hud) = self.hud.as_mut() {
            if army.is_some_and(|army| hud.army_ordered(&mut self.session, army)) {
                return;
            }
            hud.region_ordered(&mut self.session, region);
        }
    }

    fn click_map(&mut self, at: [f32; 2]) {
        match self.map.pick(self.viewport, at[0], at[1]) {
            Some(MapPick::Army { army, region }) => {
                println!("wotr-map: army {} picked in region {}", army.0, region.0);
                self.map.army_selected(army);
                self.map.set_selected_army(Some(army));
                if let Some(hud) = self.hud.as_mut() {
                    if hud.selection.pick_army_on_map(Some(army)) {
                        hud.selection.pick_every_member(&self.session.state);
                    }
                }
            }
            Some(MapPick::Region(region)) => {
                self.map.select(Some(region));
                print_region(&self.map, &self.session.state, Some(region));
                if let Some(hud) = self.hud.as_mut() {
                    hud.region_clicked(region);
                }
            }
            None => {
                self.map.select(None);
                print_region(&self.map, &self.session.state, None);
            }
        }
    }

    fn observe(&mut self) {
        let hash = self.session.state.state_hash();
        if hash != self.last_hash {
            self.last_hash = hash;
            self.session.show_on(&mut self.map);
        }
    }

    pub(crate) fn parts(&mut self) -> (&mut Session, Option<&mut Hud>, &mut WorldMap, (u32, u32)) {
        (
            &mut self.session,
            self.hud.as_mut(),
            &mut self.map,
            self.viewport,
        )
    }
}

impl App for WotrApp {
    fn input(&mut self, event: &InputEvent) {
        if let (InputEvent::Modifiers { ctrl }, Some(hud)) = (event, self.hud.as_mut()) {
            hud.ctrl = *ctrl;
        }
        if let InputEvent::Key {
            key: Key::Shift,
            pressed,
        } = event
        {
            self.map.shift_held = *pressed;
        }
        if let Some(hud) = self.hud.as_mut() {
            if hud.movies.input(event) {
                return;
            }
        }
        if let InputEvent::Key { key, pressed } = event {
            let arrow = match key {
                Key::ArrowUp => Some(ScrollKey::Up),
                Key::ArrowDown => Some(ScrollKey::Down),
                Key::ArrowLeft => Some(ScrollKey::Left),
                Key::ArrowRight => Some(ScrollKey::Right),
                _ => None,
            };
            if let Some(arrow) = arrow {
                self.map.controls.key(arrow, *pressed);
                return;
            }
            if let Some(pad) = match key {
                Key::Numpad(digit) => PadKey::from_digit(*digit),
                _ => None,
            } {
                self.map.controls.pad_key(pad, *pressed);
                return;
            }
        }
        match event {
            InputEvent::Key { key, pressed: true } => match key {
                Key::Escape => match self.hud.as_mut() {
                    Some(hud) => {
                        hud.movies.menus.set_save_enabled(self.session.saveable());
                        hud.movies.menus.toggle();
                    }
                    None => self.exit = true,
                },
                Key::Space | Key::Character('n') => {
                    self.session.play_turn();
                    self.session.show_on(&mut self.map);
                }
                Key::Character('e') => {
                    let on = self.map.toggle_ambient();
                    println!(
                        "wotr-map: ambient scenery {} ({})",
                        if on { "on" } else { "off" },
                        self.map.ambient_summary()
                    );
                }
                Key::Character('p') => {
                    println!(
                        "wotr-map: part the heavens {}",
                        if self.map.part_the_heavens() {
                            "started"
                        } else {
                            "has no rays to show"
                        }
                    );
                }
                Key::Character('m') => {
                    self.map.overlay = self.map.overlay.next();
                    println!("wotr-map: overlay {}", self.map.overlay.label());
                }
                _ => {}
            },
            InputEvent::Key { .. } => {}
            InputEvent::Scroll { lines } => self.map.camera.wheel(*lines),
            InputEvent::Resized { width, height } => self.set_viewport((*width, *height)),
            InputEvent::PointerLeft => self.map.controls.pointer_left(),
            InputEvent::PointerDoubleClick {
                button: PointerButton::Middle,
            } => self.map.controls.middle_double_click(),
            InputEvent::PointerMoved { x, y } => {
                self.pointer = (*x, *y);
                self.pointer_seen = true;
                self.map.controls.pointer_moved(*x, *y);
                self.update_hover();
            }
            InputEvent::PointerButton {
                button: PointerButton::Secondary,
                pressed,
            } => {
                self.map.controls.right_button(*pressed);
                if let Some(at) = self.click(1, *pressed) {
                    self.order_map(at);
                }
            }
            InputEvent::PointerButton {
                button: PointerButton::Middle,
                pressed,
            } => self.map.controls.middle_button(*pressed),
            InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed,
            } => {
                if let Some(at) = self.click(0, *pressed) {
                    self.click_map(at);
                }
            }
            _ => {}
        }
    }

    fn frame(&mut self, dt: f32) {
        self.map.advance(dt);
        self.update_hover();
        let started = print_events(&mut self.map);
        let dt_ms = f64::from(dt) * 1000.0;
        self.voice.place_ears(&self.map.camera.camera());
        self.voice.play_map_sounds(self.map.take_sounds());
        if let Some(hud) = self.hud.as_mut() {
            for (territory, title, text, eva) in started {
                hud.push_territory_message(&self.session.state, territory, title, text);
                if let Some(eva) = &eva {
                    self.voice.report_eva(eva);
                }
            }
            if self.session.tutorial_running() {
                for event in self.session.advance_tutorial(dt_ms, &mut self.voice) {
                    hud.tutorial_event(&event);
                }
            } else {
                self.voice.advance(dt_ms);
            }
            hud.frame(&mut self.session, std::time::Duration::from_secs_f32(dt));
            self.map.set_selected_army(hud.selection.army);
            for source in hud.take_closed_messages() {
                match source {
                    MessageSource::Territory => self.map.close_event(),
                    MessageSource::Tutorial => {
                        if self.session.tutorial_popup_closed().is_some() {
                            println!(
                                "wotr-tutorial: popup closed, the session ends back to the shell"
                            );
                            self.exit = true;
                        }
                    }
                }
            }
            for request in hud.movies.menus.take_requests() {
                match request {
                    MenuRequest::ExitMission => self.exit = true,
                    MenuRequest::SaveGame { stem, description } => {
                        match self.session.save(&stem, &description) {
                            Ok(path) => println!("wotr: saved {}", path.display()),
                            Err(error) => println!("wotr: save {stem} failed ({error})"),
                        }
                    }
                    _ => {}
                }
            }
        }
        if self.session.scripted {
            self.scripted_step();
        }
        self.observe();
    }

    fn draw(&mut self, renderer: &mut Renderer, _blend: f32) -> mint_render::Result<()> {
        if self.viewport != renderer.size() {
            self.set_viewport(renderer.size());
        }
        match self.hud.as_mut() {
            Some(hud) => {
                hud.movies.build(renderer);
                let draws = hud.movies.draws();
                self.map.draw_over(renderer, &self.session.state, &draws)
            }
            None => self.map.draw(renderer, &self.session.state),
        }
    }

    fn exit_requested(&self) -> bool {
        self.exit
    }
}

fn print_region(map: &WorldMap, state: &LivingWorldState, region: Option<RegionId>) {
    let Some(region) = region else {
        println!("wotr-map: no region under the pointer");
        return;
    };
    let name = map
        .layout
        .regions
        .get(region.index())
        .map_or("?", |r| r.name.as_str());
    let owner = state.region(region).and_then(|r| r.owner).map(|p| p.0);
    let armies: Vec<u32> = map
        .marks()
        .iter()
        .filter(|m| m.region == region)
        .map(|m| m.army.0)
        .collect();
    println!(
        "wotr-map: region {} {name} owner {owner:?} armies {armies:?}",
        region.0
    );
}

fn open_hud(
    vfs: &Arc<Vfs>,
    installs: &[PathBuf],
    session: &Session,
    renderer: &mut Renderer,
) -> Result<Option<Hud>, String> {
    let Some(local) = session.human() else {
        return Ok(None);
    };
    let (width, height) = renderer.size();
    let window = (
        f32::from(u16::try_from(width).unwrap_or(u16::MAX)),
        f32::from(u16::try_from(height).unwrap_or(u16::MAX)),
    );
    let movies = Movies::load(
        vfs,
        installs,
        Arc::clone(&session.mapped_images),
        window,
        renderer,
    )?;
    let banners: Vec<[f32; 3]> = player_colors(vfs, session)
        .iter()
        .map(|c| c.banner)
        .collect();
    let human = crate::user_options::UserOptions::load_skirmish().skirmish_user_name();
    let names: Vec<String> = session
        .state
        .players
        .iter()
        .map(|p| crate::wotr_setup::seat_name(p.kind, p.skill, &human, |key| movies.label(key)))
        .collect();
    let config = HudConfig::new(&session.data, &session.state, &banners, &names)
        .with_battle_type(session.state.battle_type);
    Ok(Some(Hud::new(movies, local, config)))
}

fn driven(vfs: &Arc<Vfs>, session: Session, args: &Args, script: &str) -> Result<(), String> {
    let steps = crate::drive::parse(script)?;
    let mut renderer = crate::open_offscreen_renderer(args.size)?;
    let mut app = WotrApp::open(&mut renderer, vfs, &args.installs, session, true)?;
    crate::drive::run(
        &mut app,
        &mut renderer,
        args.size,
        std::time::Duration::from_millis(args.drive_dt_ms),
        &steps,
        |app, verb, argument| {
            let (session, hud, map, viewport) = app.parts();
            crate::wotr_drive::step(session, hud, map, viewport, verb, argument)
        },
        |_| None,
    )
}

pub(crate) fn command(args: &[String]) -> ExitCode {
    let args = match parse(args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    let Some((primary, extras)) = args.installs.split_first() else {
        return ExitCode::FAILURE;
    };
    let vfs = Arc::new(crate::mount_primary_last(primary, extras));
    let started = if args.tutorial {
        Session::start_tutorial(&vfs, &args)
    } else {
        Session::start(&vfs, &args)
    };
    let mut session = match started {
        Ok(session) => session,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(path) = args.screenshot.clone() {
        return match screenshots(&vfs, session, &args, &path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(script) = args.drive.clone() {
        session.pump_ai();
        return match driven(&vfs, session, &args, &script) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }
    for _ in 0..args.turns {
        session.play_turn();
    }
    let config = WindowConfig {
        title: "mint - War of the Ring".to_owned(),
        ..WindowConfig::default()
    };
    let result = mint_app::run(&config, move |renderer| {
        session.pump_ai();
        WotrApp::open(renderer, &vfs, &args.installs, session, false).map_err(Into::into)
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
