use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mint_app::{InputEvent, Key, PointerButton};
use mint_render::{Renderer, ScreenDraw, UiQuad};
use mint_sage_apt_player::{ImageKey, Limits, MovieEvent, Value};
use mint_sage_apt_viewer::gadget::combo::Nav;
use mint_sage_apt_viewer::gadget::gadget_key;
use mint_sage_apt_viewer::host::{Change, GadgetClick, GadgetEvent, MovieView, SurfaceContent};
use mint_sage_apt_viewer::{ScaleMode, TextAnchor};
use mint_vfs::Vfs;

mod cah;
pub(crate) mod message_box;
mod saves;
mod update_gate;
pub(crate) mod wotr;

use crate::apt_overlay::{Hosted, ImageResolver};
use crate::game::PassabilitySources;
use crate::hud::{CellSpec, ComboSpec, ImageSource, ListSpec};
use crate::lobby::{Catalogue, Difficulty, Occupant, SkirmishSetup, SlotSetup, MAX_SLOTS};
use crate::match_stats::PlayerFigures;
use crate::post_match::{MatchSummary, SummaryPlayer, TimeLabels};
use crate::setup_grid::{Choice, Column, GridLabels};

const MAIN_MENU_MOVIE: &str = "mainmenu";
const SKIRMISH_HUB_MOVIE: &str = "skirmish";
const GAME_MAP_TYPE: (&str, &str) = ("AptMapPreview::GameMapType", "OpenPlay");
const CAH_MOVIE: &str = "createahero";
const LOAD_SCREEN_MOVIE: &str = "loadscreen";
const TIME_LINE_MOVIE: &str = "TimeLine";
const CAMPAIGN_MENU_MOVIE: &str = "CampaignMenu";
const NEXT_MISSION: &str = "OnBttnNextMission";
const LAST_MISSION: &str = "OnBttnLastMission";
const CAMPAIGN_MAIN_MENU: &str = "OnBttnMainMenu";
const CAMPAIGN_SAVE: &str = "OnBttnSaveGame";
const CAMPAIGN_LOAD: &str = "OnBttnLoadGame";
const CAMPAIGN_TAB: u8 = 1;
const LOAD_TABS: u8 = 0x0B;
const SET_PLAYER_FOCUS: &str = "SetPlayerFocus";
const CONTINUE: &str = "OnButtonContinue";
const GRAPH: &str = "AptTimeLine::RenderGraph";
const STATS_LIST: (&str, &str) = ("AptTimeLine::InitGadgets", "AptTimeLine::StatsList");
pub(crate) const BACKGROUND_MOVIE: &str = "background";
const SHELL_READY: &str = "OnInitialized";
const IN_ENGINE: &str = "InGame";
const MAIN_MENU_UNLOCK_BONUS: &str = "MainMenuUnlockBonusCampaign";
const SHOW_MAIN_MENU: &str = "ShowMainMenu";
const SHELL_BACKGROUND: &str = "ShellMapLowLOD";

pub(crate) const SHELL_CLEAR: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
const MAIN_MENU_LOGO: &str = "LogoWithShadow";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BootScreen {
    Title,
    Load,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrontEnd {
    Show,
    FadeOut,
    Cut,
}

impl FrontEnd {
    pub(crate) fn from_request(argument: &str) -> Option<Self> {
        match argument.trim().to_ascii_lowercase().as_str() {
            "fadein" => Some(Self::Show),
            "fadeout" => Some(Self::FadeOut),
            "off" => Some(Self::Cut),
            _ => None,
        }
    }

    fn call(self) -> (&'static str, Vec<Value>) {
        match self {
            Self::Show => ("ShowFrontEndBackground", Vec::new()),
            Self::FadeOut => ("HideFrontEndBackground", vec![Value::Bool(false)]),
            Self::Cut => ("HideFrontEndBackground", vec![Value::Bool(true)]),
        }
    }
}

const fn opens_front_end(screen: Screen) -> bool {
    matches!(
        screen,
        Screen::SkirmishSetup | Screen::TimeLine | Screen::CreateAHero
    )
}

const fn shell_shown(screen: Screen) -> bool {
    !matches!(screen, Screen::Loading)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct FrontEndMode {
    shown: bool,
    hidden: bool,
}

impl FrontEndMode {
    fn apply(&mut self, wanted: FrontEnd) -> Option<FrontEnd> {
        match wanted {
            FrontEnd::Show => {
                *self = Self {
                    shown: true,
                    hidden: false,
                };
                Some(FrontEnd::Show)
            }
            FrontEnd::FadeOut | FrontEnd::Cut if self.shown => {
                *self = Self {
                    shown: false,
                    hidden: true,
                };
                Some(wanted)
            }
            FrontEnd::Cut if self.hidden => {
                self.hidden = false;
                Some(FrontEnd::Cut)
            }
            FrontEnd::FadeOut | FrontEnd::Cut => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    MainMenu,
    SkirmishSetup,
    CreateAHero,
    TimeLine,
    Loading,
}

struct Score {
    mode: crate::game_mode::GameMode,
    summary: MatchSummary,
    focus: usize,
    list: Arc<ListSpec>,
    graph_focus: Vec<f32>,
    series: crate::post_match::Series,
}

pub(crate) struct PendingMatch {
    pub(crate) setup: SkirmishSetup,
    pub(crate) passability_sources: PassabilitySources,
    pub(crate) campaign: Option<crate::campaign::CampaignMatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CampaignAction {
    TimeLineContinue,
    NextMission,
    LastMission,
    MainMenu,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CampaignMenuData {
    pub(crate) mission_name: String,
    pub(crate) load_image: Option<String>,
    pub(crate) victorious: bool,
}

struct GridCell {
    row: usize,
    path: String,
    spec: Arc<ComboSpec>,
    choices: Vec<Choice>,
}

pub(crate) struct ShellApp {
    window_size: (f32, f32),
    scale_mode: ScaleMode,
    screen: Screen,
    main_menu: Hosted,
    setup: Hosted,
    cah: Hosted,
    cah_state: Option<crate::cah_screen::Shared>,
    update_gate: update_gate::UpdateGate,
    clock_ms: f64,
    screen_update_due: bool,
    cah_map_rect: Option<[f32; 4]>,
    cah_scene_texture: Option<mint_render::TextureHandle>,
    cah_suppress_promo: bool,
    cah_system: Option<Arc<crate::cah_edit::CahSystem>>,
    message_box: message_box::MessageBoxHost,
    saves: saves::Saves,
    strings: Arc<mint_sage_str::StringTable>,
    ui: crate::ui_resources::UiResources,
    vfs: Arc<Vfs>,
    load_screen: Hosted,
    time_line: Hosted,
    campaign_menu: Hosted,
    in_campaign_menu: bool,
    campaign_request: Option<(String, crate::lobby::Difficulty)>,
    tutorial_request: Option<crate::tutorial::OnTutorial>,
    campaign_action: Option<CampaignAction>,
    campaign_menu_data: Option<CampaignMenuData>,
    front_end: Hosted,
    view3d: crate::view3d::View3d,
    bink: crate::bink_surface::BinkSurfaces,
    credits: Option<(crate::credits::CreditsRoll, f32)>,
    front_end_mode: FrontEndMode,
    images: ImageResolver,
    shell_map: Option<Option<SurfaceContent>>,
    shell_map_running: bool,
    pointer: (f32, f32),
    maps: Vec<crate::mapcache::SkirmishMap>,
    map_index: usize,
    map_lists: Option<(usize, Arc<ListSpec>, Arc<ListSpec>)>,
    map_sort: crate::list_sort::ColumnSort,
    lobby: SkirmishSetup,
    catalogue: Catalogue,
    player_templates: Vec<String>,
    grid_labels: GridLabels,
    rule_choices: Arc<crate::setup_rules::RuleChoices>,
    rule_labels: crate::setup_rules::RuleLabels,
    grid: Option<Vec<GridCell>>,
    gadget_held: bool,
    pending: Option<PendingMatch>,
    campaign_still: Option<crate::campaign_still::Still>,
    load_screen_frames: u32,
    load_local_row: Option<usize>,
    game_data: Option<crate::game_data::GameData>,
    exit_requested: bool,
    costs: [MovieCost; 2],
    audio: crate::ui_audio::UiAudio,
    load_screen_music: Vec<(String, Option<String>)>,
    army_names: Vec<(String, String)>,
    mapped_images: Arc<mint_sage_ini::MappedImageIndex>,
    score: Option<Score>,
    wotr: wotr::WotrShell,
    bonus_unlocked: bool,
}

fn active_view<'a>(
    screen: &Screen,
    main_menu: &'a mut Hosted,
    setup: &'a mut Hosted,
    cah: &'a mut Hosted,
    load_screen: &'a mut Hosted,
    time_line: &'a mut Hosted,
) -> &'a mut MovieView {
    match screen {
        Screen::MainMenu => &mut main_menu.view,
        Screen::SkirmishSetup => &mut setup.view,
        Screen::CreateAHero => &mut cah.view,
        Screen::Loading => &mut load_screen.view,
        Screen::TimeLine => &mut time_line.view,
    }
}

const BACKGROUND: usize = 0;
const ACTIVE: usize = 1;

#[derive(Default)]
struct MovieCost {
    this_frame: Duration,
    total: Duration,
    worst: Duration,
    frames: u32,
}

impl MovieCost {
    fn add(&mut self, spent: Duration) {
        self.this_frame = self.this_frame.saturating_add(spent);
    }

    fn frame_done(&mut self) -> Option<(Duration, Duration)> {
        let spent = std::mem::take(&mut self.this_frame);
        self.total = self.total.saturating_add(spent);
        self.worst = self.worst.max(spent);
        self.frames = self.frames.saturating_add(1);
        if self.frames < 120 {
            return None;
        }
        let mean = self.total.checked_div(self.frames).unwrap_or_default();
        let worst = self.worst;
        *self = Self::default();
        Some((mean, worst))
    }
}

#[derive(Clone)]
pub(crate) struct ShellCache {
    ui: crate::ui_resources::UiResources,
    data: crate::game_data::GameData,
}

impl ShellApp {
    pub(crate) fn load(
        vfs: Arc<Vfs>,
        install_roots: Vec<PathBuf>,
        window_size: (f32, f32),
        scale_mode: ScaleMode,
        renderer: &mut Renderer,
        cache: Option<ShellCache>,
    ) -> Result<Box<Self>, String> {
        let (ui, cached_data) = match cache {
            Some(ShellCache { ui, data }) => (ui, Some(data)),
            None => {
                let ui = crate::ui_resources::UiResources::load(&install_roots);
                println!(
                    "shell: {} movie(s) decoded, {} localisation label(s) from {} .str file(s) \
                     ({} failed)",
                    ui.movies,
                    ui.strings.len(),
                    ui.string_files,
                    ui.string_failures,
                );
                (ui, None)
            }
        };
        crate::startup::mark("shell resources");
        let (library, fonts, strings) = (
            Arc::clone(&ui.library),
            Arc::clone(&ui.fonts),
            Arc::clone(&ui.strings),
        );
        let limits = Limits::default();
        let load_one = |name: &str, renderer: &mut Renderer| {
            MovieView::load(
                Arc::clone(&library),
                install_roots.clone(),
                name,
                Arc::clone(&fonts),
                Arc::clone(&strings),
                window_size,
                limits,
                renderer,
            )
            .map_err(|error| format!("{name}: {error}"))
        };
        let mut main_menu = Hosted::new(
            load_one(MAIN_MENU_MOVIE, renderer)?,
            MAIN_MENU_MOVIE,
            SHELL_READY,
        );
        let mut setup = Hosted::new(load_one(SKIRMISH_HUB_MOVIE, renderer)?, "setup", "");
        let mut cah = Hosted::new(load_one(CAH_MOVIE, renderer)?, "createahero", "");
        let mut message_box = message_box::MessageBoxHost::load(&load_one, renderer);
        let saves = saves::Saves::load(&load_one, renderer, scale_mode);
        let mut load_screen = Hosted::new(
            load_one(LOAD_SCREEN_MOVIE, renderer)?,
            LOAD_SCREEN_MOVIE,
            SHELL_READY,
        );
        let mut time_line = Hosted::new(
            load_one(TIME_LINE_MOVIE, renderer)?,
            TIME_LINE_MOVIE,
            SHELL_READY,
        );
        let mut campaign_menu = Hosted::new(
            load_one(CAMPAIGN_MENU_MOVIE, renderer)?,
            CAMPAIGN_MENU_MOVIE,
            SHELL_READY,
        );
        campaign_menu.view.scale_mode = scale_mode;
        campaign_menu
            .expose
            .set_extern(IN_ENGINE, Value::Bool(true));
        let mut front_end =
            Hosted::new(load_one(BACKGROUND_MOVIE, renderer)?, BACKGROUND_MOVIE, "");
        for hosted in [
            &mut main_menu,
            &mut setup,
            &mut cah,
            &mut load_screen,
            &mut time_line,
            &mut front_end,
        ] {
            hosted.view.scale_mode = scale_mode;
        }
        message_box.set_scale_mode(scale_mode);
        front_end.expose.set_extern(IN_ENGINE, Value::Bool(true));
        let bonus_unlocked = crate::user_options::UserOptions::load().bonus_unlocked();
        for screen in [
            Screen::MainMenu,
            Screen::SkirmishSetup,
            Screen::CreateAHero,
            Screen::Loading,
            Screen::TimeLine,
        ] {
            let hosted = match screen {
                Screen::TimeLine => &mut time_line,
                Screen::MainMenu => &mut main_menu,
                Screen::SkirmishSetup => &mut setup,
                Screen::CreateAHero => &mut cah,
                Screen::Loading => &mut load_screen,
            };
            declare_screen(screen, hosted, bonus_unlocked);
        }
        let maps = crate::mapcache::skirmish_maps(&vfs, &strings);
        println!(
            "shell: {} skirmish-playable map(s) found (maps/MapCache.ini, both start \
             waypoints)",
            maps.len()
        );
        crate::startup::mark("shell movies and maps");
        let audio = crate::ui_audio::UiAudio::open(Arc::clone(&vfs));
        crate::startup::mark("shell audio");
        let data = cached_data.unwrap_or_else(|| crate::load_game_data_or_exit(&vfs).0);
        crate::startup::mark("shell game data");
        let factions = crate::lobby::playable_factions(&data);
        let grid_labels = grid_labels(&data, &factions, &strings);
        let mut catalogue = Catalogue {
            random_pool: crate::lobby::random_pool(&data),
            factions,
            colours: u8::try_from(data.multiplayer_colors.len()).unwrap_or(u8::MAX),
            starts: maps
                .first()
                .map_or(2, |m| u8::try_from(m.players).unwrap_or(u8::MAX)),
            heroes: crate::setup_heroes::load_entries(&vfs, &data.hero_subclasses),
            sides: crate::lobby::template_sides(&data),
        };
        let mut lobby = default_lobby(maps.first().map(|m| m.map_path.clone()));
        lobby.fit_to_map(catalogue.starts);
        let rule_choices = Arc::clone(&data.rule_choices);
        lobby.rules = rule_choices.defaults();
        let player_templates: Vec<String> = data.players.iter().map(|p| p.name.clone()).collect();
        let mut map_index = 0;
        if let Some(preset) = crate::skirmish_preset::load(
            &crate::user_options::UserOptions::load_skirmish(),
            &player_templates,
            catalogue.heroes.len(),
        ) {
            let saved = preset
                .map
                .as_deref()
                .map(crate::skirmish_preset::same_map_key);
            if let Some((at, map)) = maps.iter().enumerate().find(|(_, m)| {
                saved.as_deref() == Some(crate::skirmish_preset::same_map_key(&m.map_path).as_str())
            }) {
                map_index = at;
                catalogue.starts = u8::try_from(map.players).unwrap_or(u8::MAX);
                lobby.map_label.clone_from(&map.map_path);
            }
            crate::skirmish_preset::apply(&preset, &mut lobby, &catalogue);
            lobby.fit_to_map(catalogue.starts);
            println!(
                "shell: skirmish setup restored from Skirmish.ini - {}",
                lobby.map_label
            );
        }
        let rule_labels = rule_labels(&strings);
        let mut images = ImageResolver::new(
            Arc::clone(&vfs),
            Arc::clone(&data.mapped_images),
            Arc::clone(&data.command_buttons),
        );
        report_gadget_templates(&mut images, renderer);
        crate::startup::mark("shell gadgets");
        let load_screen_music = data
            .players
            .iter()
            .map(|p| (p.name.clone(), p.load_screen_music.clone()))
            .collect();
        let army_names = army_names(&data, &strings);
        let mapped_images = Arc::clone(&data.mapped_images);
        Ok(Box::new(Self {
            window_size,
            scale_mode,
            screen: Screen::MainMenu,
            main_menu,
            setup,
            cah,
            cah_state: None,
            update_gate: update_gate::UpdateGate::default(),
            clock_ms: 0.0,
            screen_update_due: false,
            cah_map_rect: None,
            cah_scene_texture: None,
            cah_suppress_promo: false,
            cah_system: data.cah_system.clone(),
            message_box,
            saves,
            strings: Arc::clone(&strings),
            ui,
            vfs: Arc::clone(&vfs),
            load_screen,
            time_line,
            campaign_menu,
            in_campaign_menu: false,
            campaign_request: None,
            tutorial_request: None,
            campaign_action: None,
            campaign_menu_data: None,
            front_end,
            view3d: crate::view3d::View3d::new(Arc::clone(&vfs)),
            bink: crate::bink_surface::BinkSurfaces::new(Arc::clone(&vfs), &install_roots),
            credits: None,
            front_end_mode: FrontEndMode::default(),
            images,
            shell_map: None,
            shell_map_running: false,
            pointer: (0.0, 0.0),
            maps,
            map_index,
            map_lists: None,
            map_sort: crate::mapcache::DEFAULT_SORT,
            lobby,
            catalogue,
            player_templates,
            grid_labels,
            rule_choices,
            rule_labels,
            grid: None,
            gadget_held: false,
            pending: None,
            campaign_still: None,
            load_screen_frames: 0,
            load_local_row: None,
            game_data: Some(data),
            exit_requested: false,
            costs: Default::default(),
            audio,
            load_screen_music,
            army_names,
            mapped_images,
            score: None,
            bonus_unlocked,
            wotr: wotr::WotrShell::default(),
        }))
    }

    fn enter(&mut self, screen: Screen) {
        if screen == Screen::MainMenu {
            self.leave_strategic();
        }
        let bonus_unlocked = self.bonus_unlocked;
        let hosted = self.hosted_mut(screen);
        match hosted.restart() {
            Ok(()) => declare_screen(screen, hosted, bonus_unlocked),
            Err(error) => println!("shell: {error} - kept the previous run"),
        }
        self.screen = screen;
        if opens_front_end(screen) {
            self.set_front_end(FrontEnd::Show);
        }
        self.gadget_held = false;
        self.grid = None;
        self.load_screen_frames = 0;
        if screen != Screen::Loading {
            self.campaign_still = None;
        }
        if screen == Screen::Loading {
            self.declare_load_screen();
        }
        if screen == Screen::TimeLine {
            if self.in_campaign_menu {
                self.declare_campaign_menu();
            } else {
                self.declare_time_line();
            }
        }
        self.screen_audio(screen);
    }

    fn set_front_end(&mut self, wanted: FrontEnd) {
        if let Some(call) = self.front_end_mode.apply(wanted) {
            let (function, args) = call.call();
            self.front_end.expose.call("front end", function, args);
        }
    }

    pub(crate) fn show_score(&mut self, figures: &[PlayerFigures], setup: SkirmishSetup) {
        if let Some(index) = self.maps.iter().position(|m| {
            crate::skirmish_preset::same_map_key(&m.map_path)
                == crate::skirmish_preset::same_map_key(&setup.map_label)
        }) {
            if let Some(map) = self.maps.get(index) {
                self.map_index = index;
                self.catalogue.starts = u8::try_from(map.players).unwrap_or(u8::MAX);
            }
        }
        let resolved = setup.resolved.clone().unwrap_or_default();
        self.lobby = SkirmishSetup {
            resolved: None,
            ..setup
        };
        self.map_lists = None;
        let mut order: Vec<&crate::lobby::ResolvedPlayer> = resolved.iter().collect();
        order.sort_by_key(|player| (player.ai.is_some(), player.slot));
        let players: Vec<SummaryPlayer> = order
            .into_iter()
            .filter_map(|player| {
                let figures = figures.iter().find(|f| f.player == player.player)?.clone();
                let name = match player.ai {
                    Some(difficulty) => Difficulty::ALL
                        .iter()
                        .position(|d| *d == difficulty)
                        .and_then(|i| self.grid_labels.difficulties.get(i))
                        .cloned()
                        .unwrap_or_default(),
                    None => self.grid_labels.human.clone(),
                };
                let rgb = self
                    .grid_labels
                    .colours
                    .get(usize::from(player.colour))
                    .copied()
                    .unwrap_or([255; 3]);
                Some(SummaryPlayer { name, rgb, figures })
            })
            .collect();
        if players.is_empty() {
            self.enter(Screen::SkirmishSetup);
            return;
        }
        let summary = MatchSummary { players };
        let list = Arc::new(self.stat_list(&summary, crate::post_match::FIRST_FOCUS));
        self.score = Some(Score {
            mode: crate::game_mode::GameMode::Skirmish,
            summary,
            focus: crate::post_match::FIRST_FOCUS,
            list,
            graph_focus: Vec::new(),
            series: crate::post_match::Series::FinalScore,
        });
        self.enter(Screen::TimeLine);
    }

    fn stat_list(&self, summary: &MatchSummary, focus: usize) -> ListSpec {
        let strings = &self.strings;
        let text = |key: &str| localised(strings, key);
        let time = TimeLabels {
            days: text("TIME:Days"),
            day: text("TIME:Day"),
            hours: text("TIME:Hours"),
            hour: text("TIME:Hour"),
            minutes: text("TIME:Minutes"),
            minute: text("TIME:Minute"),
            seconds: text("TIME:Seconds"),
            second: text("TIME:Second"),
        };
        summary.stat_list(focus, &text, &time)
    }

    fn update_graph(&mut self) {
        let Some(score) = &self.score else {
            return;
        };
        #[allow(clippy::cast_possible_truncation)]
        let focus: Vec<f32> = (0..score.summary.players.len())
            .map(|i| {
                let value = self
                    .time_line
                    .view
                    .player
                    .extern_member(&format!("TimeLine:GraphFocus:{i}"));
                value.map_or(1.0, |v| v.to_number().trunc() as f32 * 0.01)
            })
            .collect();
        let series = crate::post_match::Series::named(self.time_line.view.graph_mode());
        let refocus = focus != score.graph_focus;
        let retab = series != score.series;
        if !refocus && !retab {
            return;
        }
        if refocus {
            let images = &self.mapped_images;
            let size = |name: &str| {
                images
                    .get(name)
                    .and_then(|image| image.coords)
                    .map_or([0.0; 2], |rect| {
                        #[allow(clippy::cast_precision_loss)]
                        [rect.width() as f32, rect.height() as f32]
                    })
            };
            let graph = score.summary.graph(&focus, &size);
            self.time_line.expose.image(
                ImageKey::Name(GRAPH.to_owned()),
                Some(ImageSource::Graph(Arc::new(graph))),
            );
        }
        if retab {
            let (_, y_labels) = score.summary.axis_labels(series);
            for (i, y) in y_labels.into_iter().enumerate() {
                self.time_line
                    .expose
                    .string(&format!("Timeline:YAxis:{i}"), Some(y));
            }
        }
        if let Some(score) = &mut self.score {
            score.graph_focus = focus;
            score.series = series;
        }
    }

    fn set_score_focus(&mut self, focus: usize) {
        let Some(score) = &self.score else {
            return;
        };
        let focus = focus.min(score.summary.players.len().saturating_sub(1));
        let list = Arc::new(self.stat_list(&score.summary, focus));
        if let Some(score) = &mut self.score {
            score.focus = focus;
            score.list = list;
        }
    }

    fn declare_time_line(&mut self) {
        let Some(score) = &self.score else {
            return;
        };
        let expose = &mut self.time_line.expose;
        let players = &score.summary.players;
        let screen_mode = crate::game_mode::TimeLineMode::for_match(score.mode, false, None)
            .map_or("OtherSingle", crate::game_mode::TimeLineMode::screen_mode);
        expose.set_extern("TimeLine:ScreenMode", Value::str(screen_mode));
        expose.set_extern("TimeLine:ShowSaveReplay", Value::Bool(false));
        expose.set_extern("TimeLine:NumCahAwards", Value::Number(0.0));
        expose.set_extern("TimeLine:LocalPlayerIsObserver", Value::Bool(false));
        expose.set_extern("TimeLine:StrategicEnd", Value::Bool(false));
        #[allow(clippy::cast_precision_loss)]
        expose.set_extern("TimeLine:NumOfPlayers", Value::Number(players.len() as f64));
        for (index, player) in players.iter().enumerate() {
            expose.set_extern(
                &format!("TimeLine:PlayerColor:{index}"),
                Value::str(&crate::post_match::color_extern(player.rgb)),
            );
            expose.string(
                &format!("TimeLine:PlayerName:{index}"),
                Some(player.name.clone()),
            );
            expose.string(
                &format!("TimeLine:PlayerScore:{index}"),
                Some(String::new()),
            );
            if let Some(side) = &player.figures.side {
                expose.image(
                    ImageKey::Name(format!("TimeLine:PlayerFactionIcon:{index}")),
                    Some(ImageSource::MappedImage(crate::game::faction_icon_image(
                        side,
                    ))),
                );
            }
        }
        let (x_labels, y_labels) = score.summary.axis_labels(score.series);
        for (i, (x, y)) in x_labels.into_iter().zip(y_labels).enumerate() {
            expose.string(&format!("Timeline:XAxis:{i}"), Some(x));
            expose.string(&format!("Timeline:YAxis:{i}"), Some(y));
        }
        expose.string(
            "Timeline:XAxisDescription",
            Some(localised(&self.strings, "APT:TimeDescriptionMinuteSecond")),
        );
        println!(
            "shell: TimeLine - {} player(s): {}",
            players.len(),
            players
                .iter()
                .map(|p| format!(
                    "{}/{}/{}/{:?}",
                    p.name,
                    p.figures.side.as_deref().unwrap_or("-"),
                    crate::post_match::color_extern(p.rgb),
                    p.figures.status
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    fn declare_load_screen(&mut self) {
        let Some(setup) = self.pending.as_ref().map(|p| &p.setup) else {
            return;
        };
        let Some(players) = setup.resolved.as_deref() else {
            return;
        };
        let choices = &self.rule_choices;
        let show = crate::load_screen::ShowRandom {
            template: choices.show_random_template,
            colour: choices.show_random_colour,
        };
        let army_names = &self.army_names;
        let mapped_images = &self.mapped_images;
        let rows = crate::load_screen::rows(
            setup,
            players,
            &self.grid_labels,
            show,
            &|faction| {
                army_names
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(faction))
                    .map_or_else(|| faction.to_owned(), |(_, shown)| shown.clone())
            },
            &|image| mapped_images.get(image).is_some(),
        );
        let wanted = crate::skirmish_preset::same_map_key(&setup.map_label);
        let map = self
            .maps
            .iter()
            .find(|m| crate::skirmish_preset::same_map_key(&m.map_path) == wanted);
        self.load_local_row = rows.iter().position(|r| r.local);
        let expose = &mut self.load_screen.expose;
        expose.set_extern("GameLoadingType", Value::str("_skirmish"));
        crate::load_screen::declare_rows(expose, &rows);
        expose.string("MapTitle", map.map(crate::load_screen::map_title));
        if let Some(map) = map {
            let preview =
                crate::setup_grid::preview(map, setup, &self.grid_labels, PREVIEW_BACKDROP);
            expose.image(
                ImageKey::Name(gadget_key(CURRENT_MAP.0, CURRENT_MAP.1)),
                Some(ImageSource::MapWindow(Arc::new(preview))),
            );
            expose.image(
                ImageKey::Name(gadget_key(MAP_INFO.0, MAP_INFO.1)),
                Some(ImageSource::List(Arc::new(map_info(Some(map))))),
            );
            expose.image(
                ImageKey::Name(MAP_PICTURE.to_owned()),
                Some(ImageSource::MapPicture(map.map_path.clone())),
            );
        }
        println!(
            "shell: load screen - {} row(s): {}",
            rows.len(),
            rows.iter()
                .map(|r| format!(
                    "{}/{}/{}/team {}",
                    r.name,
                    r.army,
                    format_args!("#{:02x}{:02x}{:02x}", r.colour[0], r.colour[1], r.colour[2]),
                    r.team
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    fn screen_audio(&mut self, screen: Screen) {
        match screen {
            Screen::MainMenu => self.audio.main_menu(self.shell_map_running),
            Screen::SkirmishSetup | Screen::CreateAHero => self.audio.sub_menu(),
            Screen::TimeLine => self.audio.score_screen(),
            Screen::Loading => {
                use crate::campaign_still::StillMusic;
                match self.campaign_still.as_ref().map(|still| &still.music) {
                    Some(StillMusic::SaveFile) => self.audio.loading_save_file(),
                    Some(StillMusic::Track(track)) => self.audio.loading(track.as_deref()),
                    None => {
                        let music = self.local_load_screen_music();
                        self.audio.loading(music.as_deref());
                    }
                }
            }
        }
    }

    fn local_load_screen_music(&self) -> Option<String> {
        let pending = self.pending.as_ref()?;
        let faction = &pending
            .setup
            .resolved
            .as_ref()?
            .iter()
            .find(|player| player.ai.is_none())?
            .faction;
        self.load_screen_music
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(faction))
            .and_then(|(_, music)| music.clone())
    }

    pub(crate) fn music_line(&mut self) -> String {
        self.audio.settle();
        self.audio.frame(0.0);
        self.audio.describe()
    }

    fn hosted_mut(&mut self, screen: Screen) -> &mut Hosted {
        match screen {
            Screen::MainMenu => &mut self.main_menu,
            Screen::SkirmishSetup => &mut self.setup,
            Screen::CreateAHero => &mut self.cah,
            Screen::TimeLine => &mut self.time_line,
            Screen::Loading => &mut self.load_screen,
        }
    }

    pub(crate) fn window_size(&self) -> (f32, f32) {
        self.window_size
    }

    pub(crate) fn active_stats(&self) -> mint_sage_apt_viewer::host::OverlayStats {
        match self.screen {
            Screen::MainMenu => self.main_menu.view.stats,
            Screen::SkirmishSetup => self.setup.view.stats,
            Screen::CreateAHero => self.cah.view.stats,
            Screen::TimeLine => self.time_line.view.stats,
            Screen::Loading => self.load_screen.view.stats,
        }
    }

    pub(crate) fn active_live_instances(&self) -> usize {
        match self.screen {
            Screen::MainMenu => self.main_menu.view.player.live_instances(),
            Screen::SkirmishSetup => self.setup.view.player.live_instances(),
            Screen::CreateAHero => self.cah.view.player.live_instances(),
            Screen::TimeLine => self.time_line.view.player.live_instances(),
            Screen::Loading => self.load_screen.view.player.live_instances(),
        }
    }

    pub(crate) fn active_build_profile(&self) -> mint_sage_apt_viewer::host::BuildProfile {
        match self.screen {
            Screen::MainMenu => self.main_menu.view.last_build,
            Screen::SkirmishSetup => self.setup.view.last_build,
            Screen::CreateAHero => self.cah.view.last_build,
            Screen::TimeLine => self.time_line.view.last_build,
            Screen::Loading => self.load_screen.view.last_build,
        }
    }

    pub(crate) fn force_screen(&mut self, name: &str) -> Result<(), String> {
        self.screen = match name {
            "mainmenu" => Screen::MainMenu,
            "setup" => Screen::SkirmishSetup,
            "cah" => Screen::CreateAHero,
            "load" => Screen::Loading,
            other => {
                return Err(format!(
                    "--shell-screenshot: {other:?} is not mainmenu, setup or load"
                ))
            }
        };
        if matches!(self.screen, Screen::Loading) {
            self.load_screen_frames = 1;
        }
        if opens_front_end(self.screen) {
            self.set_front_end(FrontEnd::Show);
        }
        self.screen_audio(self.screen);
        Ok(())
    }

    pub(crate) fn set_scale_mode(&mut self, mode: ScaleMode) {
        self.scale_mode = mode;
        for hosted in self.every_hosted_mut() {
            hosted.view.scale_mode = mode;
        }
        self.message_box.set_scale_mode(mode);
        self.saves.set_scale_mode(mode);
    }

    fn every_hosted_mut(&mut self) -> [&mut Hosted; 7] {
        [
            &mut self.main_menu,
            &mut self.setup,
            &mut self.cah,
            &mut self.load_screen,
            &mut self.time_line,
            &mut self.campaign_menu,
            &mut self.front_end,
        ]
    }

    pub(crate) fn set_text_anchor(&mut self, anchor: TextAnchor) {
        self.main_menu.view.text_anchor = anchor;
        self.setup.view.text_anchor = anchor;
        self.cah.view.text_anchor = anchor;
        self.message_box.set_text_anchor(anchor);
        self.saves.set_text_anchor(anchor);
        self.load_screen.view.text_anchor = anchor;
        self.time_line.view.text_anchor = anchor;
        self.campaign_menu.view.text_anchor = anchor;
    }

    pub(crate) fn take_campaign_request(&mut self) -> Option<(String, crate::lobby::Difficulty)> {
        self.campaign_request.take()
    }

    pub(crate) fn take_tutorial_request(&mut self) -> Option<crate::tutorial::OnTutorial> {
        self.tutorial_request.take()
    }

    pub(crate) fn tutorial_plan(
        &self,
        kind: crate::tutorial::TutorialKind,
    ) -> Option<crate::campaign::MissionPlan> {
        self.game_data.as_ref()?.tutorials.block(kind).plan()
    }

    pub(crate) fn take_campaign_action(&mut self) -> Option<CampaignAction> {
        self.campaign_action.take()
    }

    pub(crate) fn screen_name(&self) -> String {
        if self.in_campaign_menu {
            "CampaignMenu".to_owned()
        } else {
            format!("{:?}", self.screen)
        }
    }

    pub(crate) fn act_for_script(&mut self, action: &str) -> Result<String, String> {
        if let Some(answer) = self.act_wotr(action) {
            return answer;
        }
        if let Some(answer) = self.act_movie(action) {
            return answer;
        }
        let on_menu = self.screen == Screen::TimeLine && self.in_campaign_menu;
        let (action, said) = match action {
            "continue" if self.screen == Screen::TimeLine && self.campaign_score() => {
                let said = self
                    .score
                    .as_ref()
                    .and_then(|score| score.summary.players.first())
                    .map(|player| score_line(&player.figures))
                    .unwrap_or_default();
                (CampaignAction::TimeLineContinue, said)
            }
            "next-mission" if on_menu => (CampaignAction::NextMission, String::new()),
            "last-mission" if on_menu => (CampaignAction::LastMission, String::new()),
            "main-menu" if on_menu => (CampaignAction::MainMenu, String::new()),
            _ => return Err(format!("{action}: not offered on {}", self.screen_name())),
        };
        self.campaign_action = Some(action);
        Ok(format!("{action:?} {said}"))
    }

    fn start_credits(&mut self) {
        let strings = Arc::clone(&self.strings);
        let roll = crate::credits::Fonts::load(&self.vfs).and_then(|fonts| {
            crate::credits::CreditsRoll::load(&self.vfs, fonts, |label| strings.fetch(label))
        });
        match roll {
            Ok(roll) => {
                println!(
                    "shell: MainMenu -> Credits (AptMainMenu::Credits): {} line(s)",
                    roll.counts().0
                );
                self.credits = Some((roll, 0.0));
            }
            Err(error) => println!("shell: MainMenu -> Credits: no roll ({error})"),
        }
        self.audio.credits();
    }

    fn draw_credits(&mut self) {
        let Some((roll, clock)) = self.credits.as_mut() else {
            return;
        };
        let view = &mut self.main_menu.view;
        let Some(id) = mint_sage_apt_viewer::scene_surface::surfaces_of_type(view, |t| {
            t == crate::credits::RENDER_CREDITS
        })
        .first()
        .copied() else {
            return;
        };
        let Some([left, top, right, bottom]) = view.slot_rect(id) else {
            return;
        };
        #[allow(clippy::cast_possible_truncation)]
        let (width, height) = ((right - left) as i32, (bottom - top) as i32);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (*clock * crate::credits::UPDATES_PER_SECOND)
            .clamp(0.0, crate::credits::UPDATES_PER_SECOND) as u32;
        #[allow(clippy::cast_precision_loss)]
        let spent = steps as f32 / crate::credits::UPDATES_PER_SECOND;
        *clock = (*clock - spent).clamp(0.0, 1.0 / crate::credits::UPDATES_PER_SECOND);
        let was_finished = roll.is_finished();
        for _ in 0..steps {
            roll.update(width, height, |text, (font, points)| {
                #[allow(clippy::cast_possible_truncation)]
                let [w, h] = view
                    .measure_native_text(text, font, *points)
                    .map(|v| v.round() as i32);
                [w, h]
            });
        }
        let rows = roll.rows(width, height);
        view.set_surface(id, Some(SurfaceContent::Placed(Arc::new(rows))));
        if roll.is_finished() && !was_finished {
            println!("shell: credits roll finished - HideCredits");
            let root = view.player.root();
            let _ = view
                .player
                .call(root, crate::credits::HIDE_CREDITS, Vec::new());
        }
    }

    pub(crate) fn main_menu_command(&mut self, command: Option<&str>, args: &str) {
        match command {
            Some("Skirmish") => {
                println!(
                    "shell: MainMenu -> Skirmish (GameCode(\"Skirmish\"); the hub \
                     loads SkirmishOpenPlay itself)"
                );
                self.enter(Screen::SkirmishSetup);
            }
            Some("WarOfTheRing") => {
                println!(
                    "shell: MainMenu -> WarOfTheRing (GameCode(\"WarOfTheRing\"); \
                     the hub opens with GameMapType Strategic)"
                );
                if !self.enter_strategic() {
                    self.enter(Screen::MainMenu);
                }
            }
            Some("CreateAHero") => {
                println!("shell: MainMenu -> CreateAHero (AptMainMenu::CreateAHero)");
                self.enter_create_a_hero();
            }
            Some("ExitGame") => {
                println!("shell: MainMenu QUIT (AptMainMenu::ExitGame) - exiting");
                crate::startup::mark("quit requested");
                self.exit_requested = true;
            }
            Some("Credits") => self.start_credits(),
            Some("CreditsExit") => {
                println!("shell: MainMenu CreditsExit (AptMainMenu::CreditsExit)");
                self.credits = None;
                self.audio.main_menu(self.shell_map_running);
            }
            Some("LoadCampaign") => {
                self.open_save_load(crate::save_load::Mode::Load, CAMPAIGN_TAB);
            }
            Some("LoadGame") => {
                self.open_save_load(crate::save_load::Mode::Load, LOAD_TABS);
            }
            Some("LoadReplay") => {
                println!("shell: MainMenu LoadReplay: replays are not built");
            }
            Some("ContinueCampaign") => {
                println!("shell: MainMenu ContinueCampaign: not built yet");
            }
            Some(command) => match crate::title::current().menu_action(command) {
                Some(crate::title::MenuAction::Tutorial) => {
                    println!("shell: MainMenu -> {command}({args:?})");
                    self.tutorial_request = Some(crate::tutorial::OnTutorial::from_arg(args));
                }
                Some(crate::title::MenuAction::Campaign { key, .. }) => {
                    let difficulty = crate::campaign::difficulty_from_level(args);
                    println!(
                        "shell: MainMenu -> campaign {key} at {difficulty:?} \
                         (AptMainMenu::{command}, level {args:?})"
                    );
                    self.campaign_request = Some((key.to_owned(), difficulty));
                }
                None => {}
            },
            None => {}
        }
    }

    fn act_movie(&mut self, action: &str) -> Option<Result<String, String>> {
        let (verb, rest) = action.split_once(' ').unwrap_or((action, ""));
        if ![
            "tree", "texts", "children", "var", "press", "click", "trace", "traces",
        ]
        .contains(&verb)
        {
            return None;
        }
        let screen = self.screen;
        let player = &mut self.hosted_mut(screen).view.player;
        Some(match verb {
            "trace" => {
                player.set_trace_log(true);
                Ok("script trace output is kept".to_owned())
            }
            "traces" => Ok(player.take_traces().join("\n  ")),
            "tree" => Ok(crate::wotr_movies::tree_of(player, rest.trim()).join("\n  ")),
            "texts" => Ok(crate::wotr_movies::texts_of(player, rest.trim()).join("\n  ")),
            "children" => crate::wotr_movies::children_of(player, rest.trim()),
            "var" => crate::wotr_movies::var_of(player, rest.trim()),
            "click" => crate::wotr_movies::click_button(player, rest.trim()),
            _ => crate::wotr_movies::press_button(player, rest.trim()),
        })
    }

    fn campaign_score(&self) -> bool {
        self.score
            .as_ref()
            .is_some_and(|score| score.mode == crate::game_mode::GameMode::SinglePlayer)
    }

    pub(crate) fn start_wotr_battle(&mut self, setup: SkirmishSetup) {
        self.score = None;
        self.pending = Some(PendingMatch {
            setup,
            passability_sources: PassabilitySources::default(),
            campaign: None,
        });
        self.campaign_still = None;
        self.set_front_end(FrontEnd::FadeOut);
        self.enter(Screen::Loading);
    }

    pub(crate) fn start_campaign_mission(&mut self, plan: &crate::campaign::MissionPlan) {
        self.leave_campaign_menu();
        let mut setup = SkirmishSetup::two_player(
            String::new(),
            String::new(),
            plan.config.map.clone(),
            [false, false],
            None,
        );
        setup.slots.clear();
        setup.resolved = Some(Vec::new());
        self.score = None;
        self.pending = Some(PendingMatch {
            setup,
            passability_sources: PassabilitySources::default(),
            campaign: Some(plan.config.clone()),
        });
        self.campaign_still = Some(crate::campaign_still::Still::choose(
            plan.presentation.load_still.as_deref(),
            plan.presentation.load_music.as_deref(),
            &mut mint_sage_audio::ClientRng::from_clock(),
        ));
        self.set_front_end(FrontEnd::FadeOut);
        self.enter(Screen::Loading);
    }

    pub(crate) fn show_campaign_score(&mut self, figures: &[PlayerFigures], victory: bool) {
        self.leave_campaign_menu();
        let rgb = self
            .grid_labels
            .colours
            .first()
            .copied()
            .unwrap_or([255; 3]);
        let players: Vec<SummaryPlayer> = crate::post_match::campaign_figures(figures, victory)
            .into_iter()
            .map(|figures| SummaryPlayer {
                name: self.grid_labels.human.clone(),
                rgb,
                figures,
            })
            .collect();
        let summary = MatchSummary { players };
        let list = Arc::new(self.stat_list(&summary, crate::post_match::FIRST_FOCUS));
        self.score = Some(Score {
            mode: crate::game_mode::GameMode::SinglePlayer,
            summary,
            focus: crate::post_match::FIRST_FOCUS,
            list,
            graph_focus: Vec::new(),
            series: crate::post_match::Series::FinalScore,
        });
        self.enter(Screen::TimeLine);
    }

    pub(crate) fn show_campaign_menu(&mut self, data: CampaignMenuData) {
        self.leave_campaign_menu();
        std::mem::swap(&mut self.time_line, &mut self.campaign_menu);
        self.in_campaign_menu = true;
        self.campaign_menu_data = Some(data);
        self.score = None;
        self.enter(Screen::TimeLine);
    }

    fn leave_campaign_menu(&mut self) {
        if self.in_campaign_menu {
            std::mem::swap(&mut self.time_line, &mut self.campaign_menu);
            self.in_campaign_menu = false;
        }
    }

    pub(crate) fn campaign_to_main_menu(&mut self) {
        self.leave_campaign_menu();
        self.bonus_unlocked = crate::user_options::UserOptions::load().bonus_unlocked();
        self.score = None;
        self.enter(Screen::MainMenu);
    }

    fn declare_campaign_menu(&mut self) {
        let Some(data) = self.campaign_menu_data.clone() else {
            return;
        };
        let expose = &mut self.time_line.expose;
        expose.set_extern("CampaignMenu:Victorious", Value::Bool(data.victorious));
        if !data.mission_name.is_empty() {
            expose.string(
                "CampaignMissionName",
                Some(localised(
                    &self.strings,
                    &format!("CampaignName:{}", data.mission_name),
                )),
            );
        }
        if let Some(image) = data.load_image {
            expose.image(
                ImageKey::Name("AptCampaign::LoadImage".to_owned()),
                Some(ImageSource::MappedImage(image)),
            );
        }
    }

    pub(crate) fn take_pending(&mut self) -> Option<PendingMatch> {
        self.pending.take()
    }

    pub(crate) fn shell_map_fields(&self) -> crate::shell_map::ShellMapData {
        self.game_data
            .as_ref()
            .map(|data| data.shell_map.clone())
            .unwrap_or_default()
    }

    pub(crate) fn ui_resources(&self) -> crate::ui_resources::UiResources {
        self.ui.clone()
    }

    pub(crate) fn cache(&self) -> Option<ShellCache> {
        Some(ShellCache {
            ui: self.ui.clone(),
            data: self.game_data.clone()?,
        })
    }

    pub(crate) fn is_evil_template(&self, name: &str) -> bool {
        self.game_data
            .as_ref()
            .and_then(|d| d.players.get(name))
            .and_then(|p| p.evil)
            .unwrap_or(false)
    }

    pub(crate) fn template_side(&self, name: &str) -> Option<String> {
        self.game_data
            .as_ref()
            .and_then(|d| d.players.get(name))
            .and_then(|p| p.side.clone())
    }

    pub(crate) fn game_data_clone(&self) -> Option<crate::game_data::GameData> {
        self.game_data.clone()
    }

    pub(crate) fn take_game_data(&mut self) -> Option<crate::game_data::GameData> {
        self.game_data.take()
    }

    pub(crate) fn load_progress(&mut self, percent: u8) {
        let Some(row) = self.load_local_row else {
            return;
        };
        crate::load_screen::set_bar(&mut self.load_screen.expose, row, percent);
    }

    fn gadget_hosted(&mut self) -> &mut Hosted {
        match self.screen {
            Screen::CreateAHero => &mut self.cah,
            Screen::TimeLine => &mut self.time_line,
            _ => &mut self.setup,
        }
    }

    fn route_pointer_moved(&mut self, x: f32, y: f32) {
        if self.message_box.is_up() {
            self.message_box_pointer_moved(x, y);
            return;
        }
        if self.saves.is_open() {
            self.save_load_pointer_moved(x, y);
            return;
        }
        self.pointer = (x, y);
        match self.screen {
            Screen::MainMenu => {
                let stage = self.main_menu.view.resolve_stage_point([x, y]);
                let _ = self.main_menu.view.player.pointer_move(stage[0], stage[1]);
            }
            Screen::SkirmishSetup | Screen::CreateAHero => {
                if let Some(event) = self
                    .setup
                    .view
                    .gadget_pointer_moved([x, y], self.gadget_held)
                {
                    self.gadget_changed(&event);
                }
                if self.screen == Screen::SkirmishSetup && !self.gadget_held {
                    let _ = self.preview_pointer_moved([x, y]);
                }
                if let Some(pos) = self.gadget_hosted().view.gadget_thumb_drag_diag() {
                    crate::diag::diag!("gadget: thumb drag display_pos {pos:.1}");
                }
                if let Some(row) = self.gadget_hosted().view.gadget_hover_row_diag() {
                    crate::diag::diag!("gadget: hover row {row}");
                }
                let stage = self.gadget_hosted().view.resolve_stage_point([x, y]);
                let _ = self
                    .gadget_hosted()
                    .view
                    .player
                    .pointer_move(stage[0], stage[1]);
            }
            Screen::TimeLine => {
                let _ = self
                    .time_line
                    .view
                    .gadget_pointer_moved([x, y], self.gadget_held);
                let stage = self.time_line.view.resolve_stage_point([x, y]);
                let _ = self.time_line.view.player.pointer_move(stage[0], stage[1]);
            }
            Screen::Loading => {}
        }
    }

    fn route_pointer_button(&mut self, button: PointerButton, pressed: bool) {
        if self.message_box.is_up() {
            if button == PointerButton::Primary {
                self.message_box_pointer_button(pressed);
            }
            return;
        }
        if self.saves.is_open() {
            self.save_load_pointer_button(button, pressed);
            return;
        }
        match self.screen {
            Screen::MainMenu => {
                let stage = self
                    .main_menu
                    .view
                    .resolve_stage_point([self.pointer.0, self.pointer.1]);
                let _ = self.main_menu.view.player.pointer_move(stage[0], stage[1]);
                if button == PointerButton::Primary {
                    let _ = self.main_menu.view.player.pointer_button(pressed);
                }
            }
            Screen::SkirmishSetup | Screen::CreateAHero => {
                let at = [self.pointer.0, self.pointer.1];
                if button == PointerButton::Secondary && !pressed && !self.wotr.setup_shown() {
                    if let GadgetClick::Changed(event) =
                        self.gadget_hosted().view.gadget_right_released(at)
                    {
                        self.gadget_changed(&event);
                    }
                }
                if button != PointerButton::Primary {
                    return;
                }
                if !pressed && self.gadget_held {
                    self.gadget_held = false;
                    if let GadgetClick::Changed(event) =
                        self.gadget_hosted().view.gadget_released(at)
                    {
                        self.gadget_changed(&event);
                    }
                    return;
                }
                if pressed && self.gadget_hosted().view.gadget_pressed(at) {
                    self.gadget_held = true;
                    return;
                }
                if self.screen == Screen::SkirmishSetup && self.preview_pointer_button(at, pressed)
                {
                    return;
                }
                let stage = self.gadget_hosted().view.resolve_stage_point(at);
                let _ = self
                    .gadget_hosted()
                    .view
                    .player
                    .pointer_move(stage[0], stage[1]);
                let _ = self.gadget_hosted().view.player.pointer_button(pressed);
            }
            Screen::TimeLine => {
                if button != PointerButton::Primary {
                    return;
                }
                let at = [self.pointer.0, self.pointer.1];
                if !pressed && self.gadget_held {
                    self.gadget_held = false;
                    let _ = self.time_line.view.gadget_released(at);
                    return;
                }
                if pressed && self.time_line.view.gadget_pressed(at) {
                    self.gadget_held = true;
                    return;
                }
                let stage = self.time_line.view.resolve_stage_point(at);
                let _ = self.time_line.view.player.pointer_move(stage[0], stage[1]);
                let _ = self.time_line.view.player.pointer_button(pressed);
            }
            Screen::Loading => {}
        }
    }

    fn gadget_changed(&mut self, event: &GadgetEvent) {
        if self.screen == Screen::CreateAHero {
            self.cah_gadget_changed(event);
            return;
        }
        if self.strategic_gadget_changed(event) {
            return;
        }
        let map_list = gadget_key(MAP_LIST.0, MAP_LIST.1);
        if self.rule_changed(event) {
            return;
        }
        match (event.key.as_deref(), event.change) {
            (Some(key), Change::Row(row)) if key == map_list => {
                if let Some(map) = self.maps.get(row) {
                    println!("shell: map list - {} ({})", map.display_name, map.map_path);
                    self.map_index = row;
                    self.catalogue.starts = u8::try_from(map.players).unwrap_or(u8::MAX);
                    self.lobby.map_label.clone_from(&map.map_path);
                    self.lobby.fit_to_map(self.catalogue.starts);
                    self.grid = None;
                }
            }
            (_, Change::Row(entry)) => {
                let cell = self.grid.as_ref().and_then(|cells| {
                    cells.iter().find(|cell| {
                        self.setup
                            .expose
                            .image_target(&ImageKey::Path(cell.path.clone()))
                            == Some(event.instance)
                    })
                });
                let Some((row, choice)) =
                    cell.and_then(|cell| Some((cell.row, cell.choices.get(entry)?.clone())))
                else {
                    println!(
                        "shell: gadget {:?} row {entry} - nothing reads it",
                        event.key
                    );
                    return;
                };
                self.audio.play_misc(crate::ui_audio::COMBO_BOX_CLICK);
                let applied = self.apply_grid_choice(row, &choice);
                println!(
                    "shell: grid row {row} - {choice:?} {}",
                    if applied { "set" } else { "refused" }
                );
                self.grid = None;
            }
            (_, Change::Spot(start) | Change::SpotCleared(start)) => {
                let Ok(start) = u8::try_from(start) else {
                    return;
                };
                let changed = if matches!(event.change, Change::Spot(_)) {
                    self.lobby.click_start(start, &self.catalogue)
                } else {
                    self.lobby.clear_start(start)
                };
                let holder = self
                    .lobby
                    .slots
                    .iter()
                    .position(|s| s.occupant.occupied() && s.start == Some(start));
                println!(
                    "shell: start {start} - {} (held by slot {holder:?})",
                    if changed { "changed" } else { "unchanged" }
                );
            }
            (key, change) => println!("shell: gadget {key:?} {change:?} - nothing reads it yet"),
        }
    }

    fn grid(&mut self) -> &[GridCell] {
        let (lobby, catalogue, labels) = (&self.lobby, &self.catalogue, &self.grid_labels);
        let list = crate::setup_grid::list_path(self.setup_clip());
        self.grid.get_or_insert_with(|| {
            let mut cells = Vec::with_capacity(MAX_SLOTS * Column::ALL.len());
            for row in 0..MAX_SLOTS {
                for column in Column::ALL {
                    let (spec, choices) =
                        crate::setup_grid::cell(lobby, catalogue, labels, row, column);
                    cells.push(GridCell {
                        row,
                        path: crate::setup_grid::cell_path(&list, row, column),
                        spec: Arc::new(spec),
                        choices,
                    });
                }
            }
            cells
        })
    }

    fn sort_maps(&mut self, key: usize) {
        self.map_sort.click(key);
        let selected = self.maps.get(self.map_index).map(|m| m.map_path.clone());
        let sort = self.map_sort;
        self.maps.sort_by(|a, b| a.sort_order(b, &sort));
        if let Some(index) =
            selected.and_then(|path| self.maps.iter().position(|m| m.map_path == path))
        {
            self.map_index = index;
        }
        self.map_lists = None;
        println!("shell: map list sorted - {sort:?}");
    }

    fn map_lists(&mut self) -> (Arc<ListSpec>, Arc<ListSpec>) {
        match &self.map_lists {
            Some((index, list, info)) if *index == self.map_index => {
                (Arc::clone(list), Arc::clone(info))
            }
            _ => {
                let list = Arc::new(map_list(&self.maps, self.map_index));
                let info = Arc::new(map_info(self.maps.get(self.map_index)));
                self.map_lists = Some((self.map_index, Arc::clone(&list), Arc::clone(&info)));
                (list, info)
            }
        }
    }

    fn save_skirmish_preset(&self) {
        let mut options = crate::user_options::UserOptions::load_skirmish();
        if !crate::skirmish_preset::store(&mut options, &self.lobby, &self.player_templates) {
            return;
        }
        match options.save_skirmish() {
            Ok(path) => println!("shell: skirmish setup saved to {}", path.display()),
            Err(error) => println!("shell: could not write Skirmish.ini ({error})"),
        }
    }

    fn start_clicked(&mut self) {
        if self.maps.get(self.map_index).is_none() {
            println!(
                "shell: no skirmish-playable map found (maps/MapCache.ini missing or empty) - \
                 cannot start"
            );
            return;
        }
        if let Err(error) = self.lobby.validate_play(&self.catalogue) {
            println!("shell: PLAY refused - {error}");
            return;
        }
        if self.lobby.is_sandbox() {
            println!("shell: PLAY - one side only: GUI:SandboxMode");
        }
        println!(
            "shell: PLAY - {}, {} player(s)",
            self.lobby.map_label,
            self.lobby.players()
        );
        self.save_skirmish_preset();
        let mut setup = self.lobby.clone();
        setup.random_seed = crate::lobby::fresh_seed(setup.random_seed);
        let starts = self
            .maps
            .get(self.map_index)
            .map(|m| m.starts.as_slice())
            .unwrap_or_default();
        let players = match setup.resolve(setup.random_seed, &self.catalogue, starts) {
            Ok(players) => players,
            Err(error) => {
                println!("shell: PLAY refused - {error}");
                return;
            }
        };
        for player in &players {
            println!(
                "shell: resolved slot {} is player {}: {}, colour {}, start {}, hero {}",
                player.slot,
                player.player.get(),
                player.faction,
                player.colour,
                player.start.saturating_add(1),
                player
                    .hero
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |h| h.name_string()),
            );
        }
        setup.resolved = Some(players);
        self.pending = Some(PendingMatch {
            setup,
            passability_sources: PassabilitySources::default(),
            campaign: None,
        });
        self.campaign_still = None;
        self.set_front_end(FrontEnd::FadeOut);
        self.enter(Screen::Loading);
    }

    pub(crate) fn resize(&mut self, width: f32, height: f32) {
        self.window_size = (width, height);
        for hosted in self.every_hosted_mut() {
            hosted.view.window_size = (width, height);
        }
        self.message_box.set_window_size((width, height));
        self.saves.set_window_size((width, height));
    }

    pub(crate) fn input(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::Resized { width, height } => {
                #[allow(clippy::cast_precision_loss)]
                self.resize(width as f32, height as f32);
            }
            InputEvent::PointerMoved { x, y } => self.route_pointer_moved(x, y),
            InputEvent::PointerButton { button, pressed } => {
                self.route_pointer_button(button, pressed);
            }
            InputEvent::Scroll { lines } if self.saves.is_open() => self.save_load_scroll(lines),
            InputEvent::Key { key, pressed } if self.saves.is_open() => {
                self.save_load_key(key, pressed);
            }
            InputEvent::Scroll { lines } => {
                let at = [self.pointer.0, self.pointer.1];
                if !self.message_box.is_up()
                    && matches!(
                        self.screen,
                        Screen::SkirmishSetup | Screen::CreateAHero | Screen::TimeLine
                    )
                {
                    let _ = self.gadget_hosted().view.list_scrolled(at, lines);
                }
            }
            InputEvent::Key { key, pressed } if self.screen == Screen::CreateAHero => {
                self.cah_key(key, pressed);
            }
            InputEvent::Key { key, pressed: true }
                if self.screen == Screen::SkirmishSetup && self.setup.view.combo_open() =>
            {
                let nav = match key {
                    Key::ArrowUp => Some(Nav::Up),
                    Key::ArrowDown => Some(Nav::Down),
                    Key::Enter | Key::Space => Some(Nav::Confirm),
                    Key::Escape => Some(Nav::Cancel),
                    _ => None,
                };
                if let Some(nav) = nav {
                    if let GadgetClick::Changed(event) = self.setup.view.gadget_nav(nav) {
                        self.gadget_changed(&event);
                    }
                }
            }
            InputEvent::Key {
                key: Key::Character('-'),
                pressed: true,
            } => {
                self.set_scale_mode(self.scale_mode.toggled());
                println!("--ui-scale {}", self.scale_mode.as_str());
            }
            InputEvent::Key {
                key: Key::Character('c'),
                pressed: true,
            } => {
                let framing = self.view3d.toggle_framing();
                println!("view3d framing = {}", framing.label());
            }
            InputEvent::Key {
                key: Key::Character('/'),
                pressed: true,
            } => {
                let next = self.main_menu.view.image_modes.next();
                for hosted in [
                    &mut self.main_menu,
                    &mut self.setup,
                    &mut self.load_screen,
                    &mut self.time_line,
                    &mut self.front_end,
                ] {
                    hosted.view.image_modes = next;
                }
                println!("image modes = {}", next.label());
            }
            InputEvent::Key {
                key: Key::Character('v'),
                pressed: true,
            } => {
                let next = self.main_menu.view.text_anchor.toggled();
                self.set_text_anchor(next);
                println!("text anchor = {}", next.label());
            }
            _ => {}
        }
    }

    pub(crate) fn set_pool_gain(&self, gain: f32) {
        self.audio.set_pool_gain(gain);
    }

    pub(crate) fn frame(&mut self, dt: f32) {
        let elapsed = Duration::from_secs_f32(dt.max(0.0));
        let started = std::time::Instant::now();
        self.clock_ms += f64::from(dt.max(0.0)) * 1000.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let now_ms = self.clock_ms as u32;
        self.screen_update_due = self.update_gate.due(now_ms);
        match self.screen {
            Screen::MainMenu => {
                self.main_menu.frame(elapsed);
                for event in self.main_menu.expose.take_events() {
                    if self.played(&event) {
                        continue;
                    }
                    let args = event.args().unwrap_or_default().to_owned();
                    self.main_menu_command(event.command(), &args);
                }
            }
            Screen::SkirmishSetup => {
                if self.wotr.setup_shown() {
                    self.publish_strategic();
                }
                if self.message_box.is_up() {
                    self.message_box_frame(elapsed);
                }
                if !self.wotr.setup_shown() {
                    self.setup.expose.call(
                        "map_tab",
                        &format!("{}.MpMapAndSettings.gotoAndStop", self.setup_clip()),
                        vec![Value::str("_MapTabSelected")],
                    );
                }
                let (list, info) = self.map_lists();
                self.setup.expose.image(
                    ImageKey::Name(gadget_key(MAP_LIST.0, MAP_LIST.1)),
                    Some(ImageSource::List(list)),
                );
                self.setup.expose.image(
                    ImageKey::Name(gadget_key(MAP_INFO.0, MAP_INFO.1)),
                    Some(ImageSource::List(info)),
                );
                let strategic = self.wotr.setup_shown();
                if let Some(map) = self.maps.get(self.map_index).filter(|_| !strategic) {
                    let preview = crate::setup_grid::preview(
                        map,
                        &self.lobby,
                        &self.grid_labels,
                        PREVIEW_BACKDROP,
                    );
                    self.setup.expose.image(
                        ImageKey::Name(gadget_key(CURRENT_MAP.0, CURRENT_MAP.1)),
                        Some(ImageSource::MapWindow(Arc::new(preview))),
                    );
                }
                if !strategic {
                    let name = self
                        .maps
                        .get(self.map_index)
                        .map(|m| m.display_name.clone());
                    self.setup.expose.string(CURRENT_MAP_NAME, name);
                }
                self.declare_grid();
                if !strategic {
                    self.declare_rules();
                }
                self.setup.frame(elapsed);
                for event in self.setup.expose.take_events() {
                    if self.played(&event) {
                        continue;
                    }
                    match event.command() {
                        Some("StartGame") if self.wotr.setup_shown() => {
                            self.strategic_start_clicked();
                        }
                        Some("StartGame") => self.start_clicked(),
                        Some("Reset") => {
                            self.lobby.rules = self.rule_choices.defaults();
                            self.grid = None;
                            println!("shell: rules reset - {:?}", self.lobby.rules);
                        }
                        Some("Exit") => {
                            println!("shell: Skirmish -> MainMenu (AptSkirmish::Exit)");
                            self.enter(Screen::MainMenu);
                        }
                        Some("OnSortName") => self.sort_maps(crate::mapcache::SORT_NAME),
                        Some("OnSortPlayers") => self.sort_maps(crate::mapcache::SORT_PLAYERS),
                        Some("OnSortIcons") => self.sort_maps(crate::mapcache::SORT_HONOURS),
                        _ => {}
                    }
                }
            }
            Screen::CreateAHero => self.cah_frame(elapsed),
            Screen::TimeLine => {
                self.update_graph();
                if let Some(score) = &self.score {
                    self.time_line.expose.image(
                        ImageKey::Name(gadget_key(STATS_LIST.0, STATS_LIST.1)),
                        Some(ImageSource::List(Arc::clone(&score.list))),
                    );
                }
                self.time_line.frame(elapsed);
                for event in self.time_line.expose.take_events() {
                    if self.played(&event) {
                        continue;
                    }
                    match event.command() {
                        Some(SET_PLAYER_FOCUS) => {
                            let focus = event.args().and_then(|a| a.trim().parse::<usize>().ok());
                            if let Some(focus) = focus {
                                self.set_score_focus(focus);
                            }
                        }
                        Some(CONTINUE) if self.wotr.ending() => {
                            println!("shell: TimeLine CONTINUE - back to the main menu");
                            self.wotr.end_done();
                            self.enter(Screen::MainMenu);
                        }
                        Some(CONTINUE) if self.campaign_score() => {
                            println!("shell: TimeLine CONTINUE - campaign");
                            self.campaign_action = Some(CampaignAction::TimeLineContinue);
                        }
                        Some(CONTINUE) => {
                            println!("shell: TimeLine CONTINUE - back to the skirmish setup");
                            self.score = None;
                            self.enter(Screen::SkirmishSetup);
                        }
                        Some(NEXT_MISSION) => {
                            self.campaign_action = Some(CampaignAction::NextMission);
                        }
                        Some(LAST_MISSION) => {
                            self.campaign_action = Some(CampaignAction::LastMission);
                        }
                        Some(CAMPAIGN_MAIN_MENU) => {
                            self.campaign_action = Some(CampaignAction::MainMenu);
                        }
                        Some(CAMPAIGN_SAVE) => {
                            self.open_save_load(crate::save_load::Mode::Save, CAMPAIGN_TAB);
                        }
                        Some(CAMPAIGN_LOAD) => {
                            self.open_save_load(crate::save_load::Mode::Load, CAMPAIGN_TAB);
                        }
                        _ => {}
                    }
                }
            }
            Screen::Loading => {
                self.load_screen.frame(elapsed);
                for event in self.load_screen.expose.take_events() {
                    self.played(&event);
                }
                self.load_screen_frames = self.load_screen_frames.saturating_add(1);
            }
        }
        self.save_load_frame(elapsed);
        self.costs[ACTIVE].add(started.elapsed());
        if shell_shown(self.screen) {
            let started = std::time::Instant::now();
            self.front_end.frame(elapsed);
            for event in self.front_end.expose.take_events() {
                self.played(&event);
            }
            self.view3d.advance(elapsed);
            self.costs[BACKGROUND].add(started.elapsed());
        }
        self.advance_living_world(elapsed);
        if let Some((_, clock)) = self.credits.as_mut() {
            *clock += dt.max(0.0);
        }
        self.audio.frame(dt);
    }

    fn played(&mut self, event: &MovieEvent) -> bool {
        if event.command() == Some("SetBackground") {
            if let Some(wanted) = event.args().and_then(FrontEnd::from_request) {
                println!("shell: SetBackground -> {wanted:?}");
                self.set_front_end(wanted);
            }
            return true;
        }
        if event.command() != Some(crate::apt_overlay::PLAY_SOUND) {
            return false;
        }
        if let Some(sound) = event.args().filter(|sound| !sound.is_empty()) {
            self.audio.play_named(sound);
        }
        true
    }

    fn active_text_settled(&self) -> bool {
        match self.screen {
            Screen::MainMenu => self.main_menu.view.text_settled(),
            Screen::SkirmishSetup => self.setup.view.text_settled(),
            Screen::CreateAHero => self.cah.view.text_settled(),
            Screen::TimeLine => self.time_line.view.text_settled(),
            Screen::Loading => self.load_screen.view.text_settled(),
        }
    }

    pub(crate) fn settle_text(&mut self, renderer: &mut Renderer, max_passes: u32) -> u32 {
        let _ = self.shell_map_quad(renderer);
        self.view3d.draw(renderer, &mut self.front_end.view);
        self.view3d.draw(
            renderer,
            active_view(
                &self.screen,
                &mut self.main_menu,
                &mut self.setup,
                &mut self.cah,
                &mut self.load_screen,
                &mut self.time_line,
            ),
        );
        self.front_end.apply_images(&mut self.images, renderer);
        self.front_end.view.build(renderer);
        self.draw_living_world(renderer);
        match self.screen {
            Screen::MainMenu => self.main_menu.apply_images(&mut self.images, renderer),
            Screen::SkirmishSetup => self.setup.apply_images(&mut self.images, renderer),
            Screen::CreateAHero => self.cah.apply_images(&mut self.images, renderer),
            Screen::TimeLine => self.time_line.apply_images(&mut self.images, renderer),
            Screen::Loading => self.load_screen.apply_images(&mut self.images, renderer),
        }
        let mut passes = 0u32;
        loop {
            match self.screen {
                Screen::MainMenu => self.main_menu.view.build(renderer),
                Screen::SkirmishSetup => self.setup.view.build(renderer),
                Screen::CreateAHero => self.cah.view.build(renderer),
                Screen::TimeLine => self.time_line.view.build(renderer),
                Screen::Loading => self.load_screen.view.build(renderer),
            }
            passes = passes.saturating_add(1);
            if self.active_text_settled() || passes >= max_passes {
                break;
            }
        }
        passes
    }

    pub(crate) fn build_and_draw(&mut self, renderer: &mut Renderer) -> Vec<ScreenDraw<'_>> {
        let background = if shell_shown(self.screen) && !self.shell_map_running {
            self.shell_map_quad(renderer)
        } else {
            None
        };
        let started = std::time::Instant::now();
        self.view3d.draw(renderer, &mut self.front_end.view);
        self.front_end.apply_images(&mut self.images, renderer);
        self.front_end.view.build(renderer);
        self.costs[BACKGROUND].add(started.elapsed());
        let started = std::time::Instant::now();
        self.view3d.draw(
            renderer,
            active_view(
                &self.screen,
                &mut self.main_menu,
                &mut self.setup,
                &mut self.cah,
                &mut self.load_screen,
                &mut self.time_line,
            ),
        );
        self.bink.draw(
            renderer,
            active_view(
                &self.screen,
                &mut self.main_menu,
                &mut self.setup,
                &mut self.cah,
                &mut self.load_screen,
                &mut self.time_line,
            ),
        );
        if self.screen == Screen::MainMenu {
            self.draw_credits();
        }
        self.costs[ACTIVE].add(started.elapsed());
        let started = std::time::Instant::now();
        if self.screen == Screen::CreateAHero {
            self.scan_cah_surfaces();
        }
        self.draw_living_world(renderer);
        let movie = match self.screen {
            Screen::MainMenu => {
                self.main_menu.apply_images(&mut self.images, renderer);
                self.main_menu.view.build(renderer);
                &self.main_menu.view
            }
            Screen::SkirmishSetup => {
                self.setup.apply_images(&mut self.images, renderer);
                self.setup.view.build(renderer);
                &self.setup.view
            }
            Screen::CreateAHero => {
                self.cah.apply_images(&mut self.images, renderer);
                self.cah.view.build(renderer);
                &self.cah.view
            }
            Screen::TimeLine => {
                self.time_line.apply_images(&mut self.images, renderer);
                self.time_line.view.build(renderer);
                &self.time_line.view
            }
            Screen::Loading => {
                self.load_screen.apply_images(&mut self.images, renderer);
                self.load_screen.view.build(renderer);
                &self.load_screen.view
            }
        };
        if let Some(shown) = self.saves.shown_mut() {
            shown.apply_images(&mut self.images, renderer);
            shown.view.build(renderer);
        }
        if let Some(shown) = self.message_box.shown_mut() {
            shown.apply_images(&mut self.images, renderer);
            shown.view.build(renderer);
        }
        self.costs[ACTIVE].add(started.elapsed());
        let background_cost = self.costs[BACKGROUND].frame_done();
        let active_cost = self.costs[ACTIVE].frame_done();
        if let (Some(background), Some(active)) = (background_cost, active_cost) {
            if std::env::var_os("MINT_UI_COSTS").is_some() {
                let ms = |d: Duration| d.as_secs_f64() * 1000.0;
                println!(
                    "ui costs: {} {:.3} ms mean ({:.3} worst), Background {:.3} ms mean \
                     ({:.3} worst) over 120 frames (frame, View3D, build)",
                    match self.screen {
                        Screen::MainMenu => "MainMenu",
                        Screen::SkirmishSetup => "Skirmish",
                        Screen::CreateAHero => "CreateAHero",
                        Screen::TimeLine => "TimeLine",
                        Screen::Loading => "LoadScreen",
                    },
                    ms(active.0),
                    ms(active.1),
                    ms(background.0),
                    ms(background.1)
                );
            }
        }
        let mut all: Vec<ScreenDraw<'_>> = background.into_iter().map(ScreenDraw::Quad).collect();
        all.extend(self.front_end.view.draws());
        all.extend(movie.draws());
        if let Some(shown) = self.saves.shown() {
            all.extend(shown.view.draws());
        }
        if let Some(shown) = self.message_box.shown() {
            all.extend(shown.view.draws());
        }
        all
    }

    pub(crate) fn set_shell_map_running(&mut self, running: bool) {
        self.shell_map_running = running;
        self.screen_audio(Screen::MainMenu);
    }

    pub(crate) fn boot_draws(
        &mut self,
        renderer: &mut Renderer,
        screen: BootScreen,
    ) -> Vec<ScreenDraw<'_>> {
        let (w, h) = self.window_size;
        let mut quads = Vec::new();
        match screen {
            BootScreen::Title => {
                quads.extend(self.image_quad(
                    renderer,
                    crate::intro::TITLE_IMAGE,
                    [0.0, 0.0, w, h],
                ));
            }
            BootScreen::Load => {
                quads.extend(self.image_quad(
                    renderer,
                    crate::shell_load_screen::IMAGE,
                    [0.0, 0.0, w, h],
                ));
            }
        }
        quads.into_iter().map(ScreenDraw::Quad).collect()
    }

    pub(crate) fn campaign_still_up(&self) -> bool {
        matches!(self.screen, Screen::Loading) && self.campaign_still.is_some()
    }

    pub(crate) fn still_quad(&mut self, renderer: &mut Renderer) -> Option<UiQuad> {
        let image = self.campaign_still.as_ref()?.image.clone();
        let rows = self
            .mapped_images
            .get(&image)
            .and_then(|def| def.coords.as_ref())
            .map_or(0, mint_sage_ini::MappedRect::height);
        let rect = crate::campaign_still::still_rect(self.window_size, rows);
        self.image_quad(renderer, &image, rect)
    }

    pub(crate) fn still_foreground(&mut self, renderer: &mut Renderer) -> Option<UiQuad> {
        let (w, h) = self.window_size;
        self.image_quad(
            renderer,
            crate::campaign_still::FOREGROUND,
            [0.0, 0.0, w, h],
        )
    }

    fn image_quad(
        &mut self,
        renderer: &mut Renderer,
        name: &str,
        rect: [f32; 4],
    ) -> Option<UiQuad> {
        let content = self
            .images
            .resolve(renderer, &ImageSource::MappedImage(name.to_owned()));
        let Some(SurfaceContent::Icon(icon)) = content else {
            return None;
        };
        let mut quad = UiQuad::new(rect[0], rect[1], rect[2] - rect[0], rect[3] - rect[1]);
        quad.texture = Some(icon.texture);
        quad.uv_rect = icon.uv_rect;
        quad.uv_rotated_cw = icon.rotated;
        Some(quad)
    }

    fn shell_map_quad(&mut self, renderer: &mut Renderer) -> Option<UiQuad> {
        let content = self.shell_map.get_or_insert_with(|| {
            self.images.resolve(
                renderer,
                &ImageSource::MappedImage(SHELL_BACKGROUND.to_owned()),
            )
        });
        let Some(SurfaceContent::Icon(icon)) = *content else {
            return None;
        };
        let (w, h) = self.window_size;
        let mut quad = UiQuad::new(0.0, 0.0, w, h);
        quad.texture = Some(icon.texture);
        quad.uv_rect = icon.uv_rect;
        quad.uv_rotated_cw = icon.rotated;
        Some(quad)
    }

    fn rule_changed(&mut self, event: &GadgetEvent) -> bool {
        use crate::setup_rules::{check_id, choose, combo_id, tick, CHECKS, COMBOS, INIT};
        let Some(key) = event.key.as_deref() else {
            return false;
        };
        let rules = &mut self.lobby.rules;
        let changed = match event.change {
            Change::Row(entry) => {
                let Some(row) = (0..COMBOS).find(|r| gadget_key(INIT, &combo_id(*r)) == key) else {
                    return false;
                };
                choose(rules, row, entry)
            }
            Change::Checked(checked) => {
                let Some(row) = (0..CHECKS).find(|r| gadget_key(INIT, &check_id(*r)) == key) else {
                    return false;
                };
                tick(rules, row, checked)
            }
            _ => return false,
        };
        if changed {
            self.grid = None;
        }
        println!(
            "shell: rule {key} - {} {:?}",
            if changed { "set" } else { "unchanged" },
            self.lobby.rules
        );
        true
    }

    fn declare_rules(&mut self) {
        use crate::setup_rules::{check, check_id, combo, combo_id, CHECKS, COMBOS, INIT};
        let rules = &self.lobby.rules;
        for row in 0..COMBOS {
            let id = combo_id(row);
            let label = self.rule_labels.combos.get(row).cloned();
            self.setup.expose.string(&id, label);
            let spec = combo(rules, &self.rule_labels, row);
            self.setup.expose.image(
                ImageKey::Name(gadget_key(INIT, &id)),
                spec.map(|s| ImageSource::Combo(Arc::new(s))),
            );
        }
        for row in 0..CHECKS {
            let id = check_id(row);
            let label = self.rule_labels.checks.get(row).cloned();
            self.setup.expose.string(&id, label);
            self.setup.expose.image(
                ImageKey::Name(gadget_key(INIT, &id)),
                check(rules, row).map(ImageSource::Check),
            );
        }
    }

    fn declare_grid(&mut self) {
        let map_type = self.setup_map_type();
        let cells: Vec<(String, ImageSource)> = self
            .grid()
            .iter()
            .map(|cell| {
                let source =
                    crate::setup_grid::cell_source(map_type, cell.row, Arc::clone(&cell.spec));
                (cell.path.clone(), source)
            })
            .collect();
        for (path, source) in cells {
            self.setup.expose.image(ImageKey::Path(path), Some(source));
        }
        for row in 0..MAX_SLOTS {
            self.setup.expose.image(
                ImageKey::Name(crate::setup_grid::connection_icon(row)),
                Some(ImageSource::Hidden),
            );
        }
    }

    pub(crate) fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub(crate) fn load_screen_shown(&self) -> bool {
        matches!(self.screen, Screen::Loading) && self.load_screen_frames > 0
    }
}

const MAP_LIST: (&str, &str) = ("MpGameSetup::InitGadgets", "MapList");
const MAP_INFO: (&str, &str) = ("AptMapPreview::MapGadgetInit", "MapInfo");
const CURRENT_MAP: (&str, &str) = ("AptMapPreview::MapGadgetInit", "CurrentMap");
const PREVIEW_BACKDROP: &str = "ScrollShroud";
const MAP_PICTURE: &str = "AptMapPreview::Picture";
const CURRENT_MAP_NAME: &str = "CurrentMapName";
const MAP_LIST_COLUMNS: [(u8, bool); 3] = [(9, false), (74, false), (17, true)];

const NOT_CONQUERED: &str = "AptDifficultyNotConquered";

fn map_list(maps: &[crate::mapcache::SkirmishMap], selected: usize) -> ListSpec {
    ListSpec {
        columns: MAP_LIST_COLUMNS.to_vec(),
        rows: maps
            .iter()
            .map(|map| {
                vec![
                    CellSpec::Image {
                        image: Some(NOT_CONQUERED.to_owned()),
                        size: None,
                    },
                    CellSpec::Text(map.display_name.clone()),
                    CellSpec::Text(map.players.to_string()),
                ]
            })
            .collect(),
        selected: (selected < maps.len()).then_some(selected),
        scroll_bar: None,
        text_inset_percent: 0,
        scroll_to_top: false,
    }
}

fn score_line(figures: &PlayerFigures) -> String {
    format!(
        "{:?} at tick {}: units built/lost/killed {}/{}/{}, structures built/lost/destroyed \
         {}/{}/{}, heroes built/lost {}/{}, earned {} spent {}",
        figures.status,
        figures.end_tick,
        figures.units_built,
        figures.units_lost,
        figures.units_killed,
        figures.structures_built,
        figures.structures_lost,
        figures.structures_destroyed,
        figures.heroes_built,
        figures.heroes_lost,
        figures.earned,
        figures.spent
    )
}

fn map_info(map: Option<&crate::mapcache::SkirmishMap>) -> ListSpec {
    ListSpec {
        columns: Vec::new(),
        rows: map
            .filter(|map| !map.description.is_empty())
            .map(|map| vec![vec![CellSpec::Text(map.description.clone())]])
            .unwrap_or_default(),
        selected: None,
        scroll_bar: None,
        text_inset_percent: 0,
        scroll_to_top: true,
    }
}

const GADGET_TEMPLATES: [&str; 5] = [
    "Apt/ListBox.wnd",
    "Apt/ComboBox.wnd",
    "Apt/CheckBox.wnd",
    "Apt/HorzSlider.wnd",
    "Apt/TextEntry.wnd",
];

fn report_gadget_templates(images: &mut ImageResolver, renderer: &mut Renderer) {
    let (mut read, mut named, mut unresolved) = (0usize, 0usize, Vec::new());
    for load in GADGET_TEMPLATES {
        if let Some(report) = images.template_report(renderer, load) {
            read += 1;
            named += report.named.len();
            unresolved.extend(report.unresolved);
        }
    }
    println!(
        "gadgets: {read} of {} template(s) read, {} of {named} image name(s) resolve{}",
        GADGET_TEMPLATES.len(),
        named.saturating_sub(unresolved.len()),
        if unresolved.is_empty() {
            String::new()
        } else {
            format!(" (unresolved: {})", unresolved.join(", "))
        }
    );
}

fn declare_screen(screen: Screen, hosted: &mut Hosted, bonus_unlocked: bool) {
    hosted.expose.set_extern(IN_ENGINE, Value::Bool(true));
    match screen {
        Screen::MainMenu => {
            hosted.expose.fire(SHOW_MAIN_MENU, Vec::new());
            if crate::title::current().unlocks_bonus {
                hosted.expose.set_extern(
                    MAIN_MENU_UNLOCK_BONUS,
                    Value::str(if bonus_unlocked { "1" } else { "0" }),
                );
            }
            hosted.expose.image(
                ImageKey::Path("Image".to_owned()),
                Some(ImageSource::MappedImage(MAIN_MENU_LOGO.to_owned())),
            );
        }
        Screen::SkirmishSetup => {
            hosted
                .expose
                .set_extern("AptMpGameRules::ShowChat", Value::Bool(false));
            hosted
                .expose
                .set_extern("AptMpGameRules::ShowClans", Value::Bool(false));
            hosted
                .expose
                .set_extern(GAME_MAP_TYPE.0, Value::str(GAME_MAP_TYPE.1));
            hosted
                .expose
                .set_extern("MpGameSetup::HostMode", Value::str("1"));
            #[allow(clippy::cast_precision_loss)]
            for (name, rows) in [
                ("MpGameRules::NumComboBoxes", crate::setup_rules::COMBOS),
                ("MpGameRules::NumCheckBoxes", crate::setup_rules::CHECKS),
            ] {
                hosted.expose.set_extern(name, Value::Number(rows as f64));
            }
        }
        Screen::TimeLine | Screen::CreateAHero | Screen::Loading => {}
    }
}

fn default_lobby(map_label: Option<String>) -> SkirmishSetup {
    let seat = |occupant| SlotSetup {
        hero: crate::lobby::HeroPick::Random,
        ..SlotSetup::new(occupant)
    };
    let mut slots = vec![seat(Occupant::Human), seat(Occupant::Ai(Difficulty::Easy))];
    slots.resize(MAX_SLOTS, SlotSetup::closed());
    SkirmishSetup {
        map_label: map_label.unwrap_or_default(),
        slots,
        rules: crate::lobby::MatchRules::default(),
        army: None,
        spawn: None,
        spawn_buildings: None,
        dummies: None,
        dummies_attack: false,
        drive_templates: Vec::new(),
        wotr: None,
        cah_hero: None,
        random_seed: crate::skirmish::SKIRMISH_SEED,
        resolved: None,
    }
}

fn rule_labels(strings: &mint_sage_str::StringTable) -> crate::setup_rules::RuleLabels {
    let text = |key: &str| {
        let found = mint_sage_apt_viewer::localise(&format!("${key}"), strings);
        if found.starts_with("MISSING:") {
            key.to_owned()
        } else {
            found
        }
    };
    crate::setup_rules::RuleLabels {
        combos: crate::setup_rules::COMBO_KEYS.map(text),
        checks: crate::setup_rules::CHECK_KEYS.map(text),
        factors: crate::setup_rules::COMMAND_POINT_FACTORS
            .iter()
            .map(|(_, key)| text(key))
            .collect(),
        default_suffix: default_suffix(strings),
    }
}

fn default_suffix(strings: &mint_sage_str::StringTable) -> String {
    let found = mint_sage_apt_viewer::localise("$VALUE:Default", strings);
    if found.starts_with("MISSING:") {
        String::new()
    } else {
        found
    }
}

pub(crate) fn localised(strings: &mint_sage_str::StringTable, key: &str) -> String {
    let found = mint_sage_apt_viewer::localise(&format!("${key}"), strings);
    if found.starts_with("MISSING:") {
        key.to_owned()
    } else {
        found
    }
}

fn army_names(
    data: &crate::game_data::GameData,
    strings: &mint_sage_str::StringTable,
) -> Vec<(String, String)> {
    data.players
        .iter()
        .filter(|def| def.playable == Some(true))
        .map(|def| {
            (
                def.name.clone(),
                localised(strings, &format!("INI:{}", def.name)),
            )
        })
        .collect()
}

fn grid_labels(
    data: &crate::game_data::GameData,
    factions: &[String],
    strings: &mint_sage_str::StringTable,
) -> GridLabels {
    let text = |key: &str| localised(strings, key);
    GridLabels {
        human: text("GUI:Player"),
        closed: text("GUI:Closed"),
        difficulties: Difficulty::ALL.map(|d| text(d.label())),
        random: text("GUI:Random"),
        none: text("Team:0"),
        teams: ["Team:1", "Team:2", "Team:3", "Team:4"].map(text),
        factions: factions
            .iter()
            .map(|name| text(&format!("INI:{name}")))
            .collect(),
        colours: (0..data.multiplayer_colors.len())
            .filter_map(|i| data.multiplayer_colors.nth(i))
            .map(|(_, def)| def.rgb.map_or([255; 3], |rgb| [rgb.r, rgb.g, rgb.b]))
            .collect(),
        random_colour: RANDOM_COLOUR.to_owned(),
        hero_default_suffix: default_suffix(strings),
        numbers: (1..=MAX_SLOTS)
            .map(|n| text(&format!("NUMBER:{n}")))
            .collect(),
    }
}

const RANDOM_COLOUR: &str = "AptRandomColor";

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod map_list_tests {
    use super::{map_info, map_list, MAP_LIST_COLUMNS, NOT_CONQUERED};
    use crate::hud::CellSpec;
    use crate::mapcache::SkirmishMap;

    fn map(name: &str, players: u32, description: &str) -> SkirmishMap {
        SkirmishMap {
            map_path: format!("maps/{name}.map"),
            display_name: name.to_owned(),
            players,
            description: description.to_owned(),
            extent: None,
            starts: Vec::new(),
            official: true,
        }
    }

    #[test]
    fn the_map_list_is_star_name_and_players_with_the_selection() {
        let maps = [map("Rivendell", 3, ""), map("Arnor", 4, "North.")];
        let list = map_list(&maps, 1);
        assert_eq!(list.columns, MAP_LIST_COLUMNS.to_vec());
        assert_eq!(list.selected, Some(1));
        assert_eq!(
            list.rows[1],
            vec![
                CellSpec::Image {
                    image: Some(NOT_CONQUERED.to_owned()),
                    size: None
                },
                CellSpec::Text("Arnor".to_owned()),
                CellSpec::Text("4".to_owned()),
            ]
        );
        assert_eq!(list.rows.len(), 2);
        assert_eq!(map_list(&maps, 2).selected, None);
    }

    #[test]
    fn the_description_is_one_row() {
        let arnor = map("Arnor", 4, "This was once the realm.");
        let info = map_info(Some(&arnor));
        assert_eq!(
            info.rows,
            vec![vec![CellSpec::Text("This was once the realm.".to_owned())]]
        );
        assert!(map_info(Some(&map("Umbar", 2, ""))).rows.is_empty());
        assert!(map_info(None).rows.is_empty());
    }
}

#[cfg(test)]
mod front_end_tests {
    use super::{opens_front_end, shell_shown, FrontEnd, FrontEndMode, Screen};
    use mint_sage_apt_player::Value;

    #[test]
    fn a_set_background_request_names_one_of_three_things() {
        assert_eq!(FrontEnd::from_request("fadein"), Some(FrontEnd::Show));
        assert_eq!(FrontEnd::from_request(" FadeOut "), Some(FrontEnd::FadeOut));
        assert_eq!(FrontEnd::from_request("off"), Some(FrontEnd::Cut));
        assert_eq!(FrontEnd::from_request("on"), None);
    }

    #[test]
    fn each_request_is_the_background_movies_own_call() {
        assert_eq!(FrontEnd::Show.call(), ("ShowFrontEndBackground", vec![]));
        assert_eq!(
            FrontEnd::FadeOut.call(),
            ("HideFrontEndBackground", vec![Value::Bool(false)])
        );
        assert_eq!(
            FrontEnd::Cut.call(),
            ("HideFrontEndBackground", vec![Value::Bool(true)])
        );
    }

    #[test]
    fn a_hide_acts_only_on_a_shown_background() {
        let mut mode = FrontEndMode::default();
        assert_eq!(mode.apply(FrontEnd::FadeOut), None);
        assert_eq!(mode.apply(FrontEnd::Show), Some(FrontEnd::Show));
        assert_eq!(mode.apply(FrontEnd::FadeOut), Some(FrontEnd::FadeOut));
        assert_eq!(mode.apply(FrontEnd::FadeOut), None);
        assert_eq!(mode.apply(FrontEnd::Cut), Some(FrontEnd::Cut));
        assert_eq!(mode.apply(FrontEnd::Cut), None);
    }

    #[test]
    fn screens_ask_for_the_background_on_opening() {
        assert!(opens_front_end(Screen::SkirmishSetup) && opens_front_end(Screen::TimeLine));
        assert!(!opens_front_end(Screen::MainMenu) && !opens_front_end(Screen::Loading));
        assert!(shell_shown(Screen::TimeLine) && !shell_shown(Screen::Loading));
    }
}
