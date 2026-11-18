use std::sync::Arc;

use mint_app::{InputEvent, Key};
use mint_movie::{PlayFlags, VideoSurface};
use mint_render::Renderer;
use mint_sage_audio::SageAudio;
use mint_sage_campaign::{MissionEnd, Outcome, TimeLineClose};
use mint_sage_ini::{BlockTable, Document, LinearCampaignIndex};
use mint_sage_movie::{MovieLibrary, MovieRoots, MovieSession};
use mint_vfs::Vfs;

use crate::campaign::CampaignRun;
use crate::campaign_dev::{self, CampaignDev};
use crate::game::MatchConfig;
use crate::match_start::{MatchStartPresentation, StartTail, TailAction};
use crate::movie_fade::MovieFade;
use crate::shell::{CampaignAction, CampaignMenuData};
use crate::transition::{Groups, TransitionHandler, POOLS};
use crate::tutorial::{OnTutorial, TutorialKind};
use crate::{AppScreen, AppState};

pub(crate) const LANGUAGE: &str = "english";
pub(crate) const FADE_IN_GAME_MOVIE: &str = "FadeInGameMovie";

pub(crate) struct MoviePlayback {
    session: MovieSession,
    surface: VideoSurface,
    audio: Option<SageAudio>,
    texture: Option<mint_render::TextureHandle>,
    started: bool,
    skippable: bool,
    end_fade: bool,
    fade: MovieFade,
}

#[derive(Default)]
pub(crate) struct CampaignFlow {
    run: Option<CampaignRun>,
    tail: Option<StartTail>,
    tail_game: Option<Box<crate::game::Game>>,
    presentation: MatchStartPresentation,
    movie: Option<MoviePlayback>,
    after_movie: Option<Outcome>,
    transitions: Option<TransitionHandler>,
    library: Option<MovieLibrary>,
    dev: Option<CampaignDev>,
    dev_tutorial: Option<TutorialKind>,
    skip_movies: bool,
    logged_gains: [f32; POOLS],
}

fn read_documents(vfs: &Vfs, suffix: &str) -> Vec<Document> {
    let mut paths: Vec<String> = vfs
        .paths_under("data/ini")
        .map(|path| path.as_str().to_owned())
        .filter(|path| path.ends_with(suffix))
        .collect();
    paths.sort();
    let table = BlockTable::corpus_path();
    paths
        .iter()
        .filter_map(|path| {
            let bytes = vfs.read(path).ok()?;
            Document::parse(path, &String::from_utf8_lossy(&bytes), &table).ok()
        })
        .collect()
}

pub(crate) fn linear_campaigns(vfs: &Vfs) -> LinearCampaignIndex {
    let mut index = LinearCampaignIndex::new();
    let mut paths: Vec<String> = vfs
        .paths_under("data/ini")
        .map(|path| path.as_str().to_owned())
        .filter(|path| {
            path.rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with("linearcampaign") && name.ends_with(".ini"))
        })
        .collect();
    paths.sort();
    let table = BlockTable::corpus_path();
    for path in &paths {
        let Ok(bytes) = vfs.read(path) else { continue };
        match Document::parse(path, &String::from_utf8_lossy(&bytes), &table) {
            Ok(document) => index.insert_document(&document),
            Err(error) => println!("campaign: {path} unreadable ({error})"),
        }
    }
    index
}

impl CampaignFlow {
    pub(crate) fn set_dev_entry(&mut self, dev: CampaignDev) {
        self.dev = Some(dev);
    }

    pub(crate) fn set_dev_tutorial(&mut self, kind: TutorialKind) {
        self.dev_tutorial = Some(kind);
    }

    pub(crate) fn set_skip_movies(&mut self, skip: bool) {
        self.skip_movies = skip;
    }

    pub(crate) fn handler(&mut self, vfs: &Vfs) -> &mut TransitionHandler {
        self.transitions.get_or_insert_with(|| {
            let mut groups = Groups::default();
            for document in read_documents(vfs, "/windowtransitions.ini") {
                groups.insert_document(&document);
            }
            println!("transitions: {} group(s) read", groups.len());
            TransitionHandler::new(groups)
        })
    }

    pub(crate) fn advance(&mut self, dt: f32) {
        if self.movie.is_some() {
            return;
        }
        if let Some(handler) = &mut self.transitions {
            handler.advance(dt);
        }
    }

    pub(crate) fn cover_alpha(&self) -> f32 {
        self.transitions
            .as_ref()
            .map_or(0.0, TransitionHandler::screen_alpha)
    }

    pub(crate) fn movie_playing(&self) -> bool {
        self.movie.is_some()
    }

    pub(crate) fn blocks_simulation(&self) -> bool {
        self.movie.is_some() || self.tail.as_ref().is_some_and(StartTail::blocks_simulation)
    }

    pub(crate) fn movie_library(&mut self, vfs: &Vfs) -> &MovieLibrary {
        self.library.get_or_insert_with(|| MovieLibrary::load(vfs))
    }

    pub(crate) fn resize(&mut self, event: &InputEvent) {
        if let Some(game) = self.tail_game.as_mut() {
            mint_app::App::input(game.as_mut(), event);
        }
    }

    pub(crate) fn movie_input(&mut self, event: &InputEvent) -> bool {
        let Some(movie) = &mut self.movie else {
            return false;
        };
        if let InputEvent::Key {
            key: Key::Escape,
            pressed: false,
        } = event
        {
            if movie.skippable && movie.end_fade {
                movie.fade.latch();
            } else if movie.skippable && movie.session.skip(movie.audio.as_mut()) {
                println!("campaign: movie skipped");
            }
        }
        true
    }
}

impl AppState {
    pub(crate) fn play_movie(
        &mut self,
        renderer: &mut Renderer,
        name: &str,
        skippable: bool,
        end_fade: bool,
    ) -> bool {
        if self.campaign_flow.skip_movies {
            println!("campaign: movie {name} skipped (scripted run)");
            return false;
        }
        let flow = &mut self.campaign_flow;
        let vfs = &self.vfs;
        let library = flow.library.get_or_insert_with(|| MovieLibrary::load(vfs));
        let roots = MovieRoots::new(&self.install_roots, LANGUAGE, None);
        let session =
            match MovieSession::open(name, library, &roots, skippable, PlayFlags::default()) {
                Ok(session) => session,
                Err(error) => {
                    println!("campaign: movie {name} not played: {error}");
                    return false;
                }
            };
        let surface = match VideoSurface::new(renderer) {
            Ok(surface) => surface,
            Err(error) => {
                println!("campaign: movie {name}: no video surface ({error})");
                return false;
            }
        };
        println!("campaign: playing movie {name} (skippable {skippable})");
        flow.movie = Some(MoviePlayback {
            session,
            surface,
            audio: crate::movie_cmd::open_audio(vfs),
            texture: None,
            started: false,
            skippable,
            end_fade,
            fade: MovieFade::default(),
        });
        true
    }

    pub(crate) fn apply_sound_fade(&mut self) {
        let Some(handler) = &self.campaign_flow.transitions else {
            return;
        };
        let gains: [f32; POOLS] = std::array::from_fn(|pool| handler.pool_gain(pool));
        let [tactical, _, shell] = gains;
        match &self.screen {
            AppScreen::Shell(shell_screen) => shell_screen.set_pool_gain(shell),
            AppScreen::Match(game) => game.set_audio_pool_gain(tactical),
        }
        if let Some(game) = &self.campaign_flow.tail_game {
            game.set_audio_pool_gain(tactical);
        }
        if let Some(audio) = self
            .campaign_flow
            .movie
            .as_ref()
            .and_then(|movie| movie.audio.as_ref())
        {
            audio.set_pool_gain(shell);
        }
        if gains != self.campaign_flow.logged_gains {
            self.campaign_flow.logged_gains = gains;
            println!(
                "sound fade: pool gains tactical {tactical:.4} world {:.4} shell {shell:.4}",
                gains[1]
            );
        }
    }

    pub(crate) fn draw_movie(
        &mut self,
        renderer: &mut Renderer,
    ) -> Option<mint_render::Result<()>> {
        let vfs = Arc::clone(&self.vfs);
        let movie = self.campaign_flow.movie.as_mut()?;
        if !movie.started {
            movie.started = true;
            movie
                .session
                .start(movie.audio.as_mut().map(|audio| (audio, vfs.as_ref())));
        }
        let tick = movie.session.update(movie.audio.as_mut());
        let frames = u64::from(movie.session.player().info().frame_count);
        let shown = u64::from(movie.session.player().frames_shown());
        let end_fade = movie.end_fade;
        let mut fade = movie.fade;
        let handler = self.campaign_flow.handler(&vfs);
        let at_mark = shown.saturating_add(handler.group_length(FADE_IN_GAME_MOVIE)) >= frames;
        let stop = !tick.done && fade.step(handler, end_fade, at_mark, tick.rendered);
        let alpha = self.campaign_flow.cover_alpha();
        let movie = self.campaign_flow.movie.as_mut()?;
        movie.fade = fade;
        let result = crate::movie_cmd::present(
            renderer,
            &mut movie.surface,
            &movie.session,
            tick.rendered || alpha > 0.0,
            &mut movie.texture,
            alpha,
        );
        if stop {
            movie.session.skip(movie.audio.as_mut());
            println!("campaign: movie skipped");
        }
        if tick.done || stop {
            println!("campaign: movie finished");
            if let Some(movie) = self.campaign_flow.movie.take() {
                if movie.end_fade && !movie.fade.started() {
                    let handler = self.campaign_flow.handler(&vfs);
                    handler.set_group_ended(FADE_IN_GAME_MOVIE);
                    handler.set_hold(true);
                }
            }
        }
        Some(result)
    }

    pub(crate) fn campaign_step(&mut self, renderer: &mut Renderer) {
        let (request, action) = match &mut self.screen {
            AppScreen::Shell(shell) => {
                (shell.take_campaign_request(), shell.take_campaign_action())
            }
            AppScreen::Match(_) => (None, None),
        };
        if let Some((key, difficulty)) = request {
            self.begin_campaign(renderer, &key, difficulty, 0);
        }
        let tutorial = match &mut self.screen {
            AppScreen::Shell(shell) => shell.take_tutorial_request(),
            AppScreen::Match(_) => None,
        };
        match tutorial {
            Some(OnTutorial::Start(kind)) => self.begin_tutorial(kind),
            Some(OnTutorial::WarOfTheRing) => {
                println!("tutorial: Strategic is the War of the Ring tutorial, which is not built");
            }
            Some(OnTutorial::Nothing) | None => {}
        }
        if matches!(self.screen, AppScreen::Shell(_)) {
            if let Some(kind) = self.campaign_flow.dev_tutorial.take() {
                println!(
                    "tutorial-dev: {} map {:?}",
                    kind.label(),
                    self.tutorial_map(kind).unwrap_or_default()
                );
                self.begin_tutorial(kind);
            }
        }
        if matches!(self.screen, AppScreen::Shell(_)) {
            if let Some(dev) = self.campaign_flow.dev.take() {
                self.begin_dev_entry(renderer, &dev);
            }
        }
        if let Some(action) = action {
            self.campaign_action(action);
        }
        let scripted_movie = match &mut self.screen {
            AppScreen::Match(game) if self.campaign_flow.movie.is_none() => {
                game.take_script_movie()
            }
            _ => None,
        };
        if let Some(movie) = scripted_movie {
            self.play_movie(renderer, &movie.name, movie.skippable, false);
        }
        let exit = match &mut self.screen {
            AppScreen::Shell(shell) => shell.take_save_exit(),
            AppScreen::Match(_) => None,
        };
        match exit {
            Some(crate::save_load::Exit::Load(path)) => self.load_campaign_save(&path),
            Some(crate::save_load::Exit::Save { stem, description }) => {
                self.save_campaign(&stem, &description);
            }
            Some(crate::save_load::Exit::Closed) | None => {}
        }
        if self.campaign_flow.movie.is_none() {
            if let Some(outcome) = self.campaign_flow.after_movie.take() {
                self.run_outcome(outcome);
            }
        }
        self.tail_step(renderer);
    }

    fn tail_step(&mut self, renderer: &mut Renderer) {
        let Some(mut tail) = self.campaign_flow.tail.take() else {
            return;
        };
        let playing = self.campaign_flow.movie.is_some();
        let frame = match &self.screen {
            AppScreen::Match(game) => game.sim_frame(),
            AppScreen::Shell(_) => 0,
        };
        let vfs = Arc::clone(&self.vfs);
        let action = tail.tick(self.campaign_flow.handler(&vfs), playing, frame);
        let mut finished = false;
        match action {
            TailAction::DropLoadScreen => {
                if let Some(game) = self.campaign_flow.tail_game.take() {
                    self.screen = AppScreen::Match(game);
                }
            }
            TailAction::PlayMovie(name) => {
                self.play_movie(renderer, &name, true, true);
            }
            TailAction::Finished => {
                println!("campaign: start tail finished at frame {frame}");
                self.campaign_flow.presentation.reset();
                finished = true;
            }
            TailAction::None => {}
        }
        if !finished {
            self.campaign_flow.tail = Some(tail);
        }
    }

    pub(crate) fn begin_start_tail(
        &mut self,
        config: &MatchConfig,
        game: Box<crate::game::Game>,
    ) -> Result<(), Box<crate::game::Game>> {
        let MatchConfig::Campaign(campaign) = config else {
            return Err(game);
        };
        let vfs = Arc::clone(&self.vfs);
        let presentation = self.campaign_flow.presentation.clone();
        let tail = StartTail::begin(
            campaign.mode,
            &presentation,
            self.campaign_flow.handler(&vfs),
        );
        match tail {
            Some(tail) => {
                self.campaign_flow.tail = Some(tail);
                self.campaign_flow.tail_game = Some(game);
                Ok(())
            }
            None => Err(game),
        }
    }

    fn begin_campaign(
        &mut self,
        renderer: &mut Renderer,
        key: &str,
        difficulty: crate::lobby::Difficulty,
        start_index: u32,
    ) {
        let index = linear_campaigns(&self.vfs);
        let Some(mut run) = CampaignRun::new(
            &index,
            key,
            difficulty,
            crate::title::current().unlocks_bonus,
        ) else {
            println!("campaign: no LinearCampaign named {key} in this install");
            return;
        };
        println!(
            "campaign: {key} - {} mission(s), difficulty {difficulty:?}",
            run.def.missions.len()
        );
        let outcome = run.state.start_at(start_index);
        let intro = run
            .def
            .overall_intro_movie
            .clone()
            .filter(|_| start_index == 0);
        self.campaign_flow.run = Some(run);
        match intro {
            Some(name) if self.play_movie(renderer, &name, true, false) => {
                self.campaign_flow.after_movie = Some(outcome);
            }
            _ => self.run_outcome(outcome),
        }
    }

    fn tutorial_map(&self, kind: TutorialKind) -> Option<String> {
        match &self.screen {
            AppScreen::Shell(shell) => shell.tutorial_plan(kind).map(|plan| plan.config.map),
            AppScreen::Match(_) => None,
        }
    }

    fn begin_tutorial(&mut self, kind: TutorialKind) {
        let plan = match &self.screen {
            AppScreen::Shell(shell) => shell.tutorial_plan(kind),
            AppScreen::Match(_) => None,
        };
        let Some(plan) = plan else {
            println!("tutorial: {} names no map - nothing starts", kind.label());
            return;
        };
        println!(
            "tutorial: {} on {} (mode 6, difficulty {:?}, intro movie {:?}, fade-up {} frames)",
            kind.label(),
            plan.config.map,
            plan.config.difficulty,
            plan.presentation.intro_movie,
            plan.presentation.fade_up_frames
        );
        self.campaign_flow.run = None;
        self.campaign_flow.presentation = plan.presentation.clone();
        if let AppScreen::Shell(shell) = &mut self.screen {
            shell.start_campaign_mission(&plan);
        }
    }

    fn begin_dev_entry(&mut self, renderer: &mut Renderer, dev: &CampaignDev) {
        let index = linear_campaigns(&self.vfs);
        let resolved = campaign_dev::resolve_campaign(&index, &dev.campaign).and_then(|def| {
            let at = campaign_dev::resolve_mission(def, dev.mission.as_ref())?;
            Ok((def, at))
        });
        let (def, at) = match resolved {
            Ok(found) => found,
            Err(error) => {
                println!("campaign-dev: {error}");
                return;
            }
        };
        let mission = def.missions.get(usize::try_from(at).unwrap_or(usize::MAX));
        println!(
            "campaign-dev: campaign {} mission {}/{} {:?} map {:?} difficulty {}",
            def.name,
            at.saturating_add(1),
            def.missions.len(),
            mission.map(|m| m.name.as_str()).unwrap_or_default(),
            mission.and_then(|m| m.map.as_deref()).unwrap_or_default(),
            campaign_dev::difficulty_label(dev.difficulty)
        );
        let key = def.name.clone();
        self.begin_campaign(renderer, &key, dev.difficulty, at);
    }

    fn read_campaign_save(&self, path: &std::path::Path) -> Result<CampaignRun, String> {
        use mint_sage_save::{CampaignBlock, SaveFile, CHUNK_CAMPAIGN};
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        let info = mint_sage_save::read_info(&bytes).map_err(|error| error.to_string())?;
        if info.save_file_type != 1 {
            return Err(format!(
                "save type {} holds a mission in progress, which mint does not load",
                info.save_file_type
            ));
        }
        let file = SaveFile::parse(&bytes).map_err(|error| error.to_string())?;
        let (at, block) = file
            .find(CHUNK_CAMPAIGN)
            .ok_or("the file has no CHUNK_Campaign block")?;
        let offset = file.payload_offset(at).ok_or("no payload offset")?;
        let campaign = CampaignBlock::decode(block.payload(), offset)
            .map_err(|error| error.to_string())?
            .campaign
            .ok_or("the file holds no campaign run")?;
        let name = campaign.name.to_string_lossy();
        CampaignRun::from_save(
            &linear_campaigns(&self.vfs),
            &name,
            campaign.difficulty,
            crate::title::current().unlocks_bonus,
            &campaign.state,
        )
        .ok_or_else(|| format!("this install defines no campaign {name}"))
    }

    pub(crate) fn load_campaign_save(&mut self, path: &std::path::Path) {
        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        match self.read_campaign_save(path) {
            Ok(mut run) => {
                let outcome = run.state.load();
                println!(
                    "campaign: loaded {file} - {} at mission {}",
                    run.def.name,
                    run.state.index()
                );
                self.campaign_flow.run = Some(run);
                self.run_outcome(outcome);
            }
            Err(error) => {
                println!("campaign: could not load {file}: {error}");
                if let AppScreen::Shell(shell) = &mut self.screen {
                    shell.show_load_error(&file);
                }
            }
        }
    }

    fn save_campaign(&mut self, stem: &str, description: &str) {
        let Some(run) = &self.campaign_flow.run else {
            println!("campaign: save {stem}: no campaign is running");
            return;
        };
        match crate::campaign_save::write_user_save(&self.install_roots, run, stem, description) {
            Ok(path) => println!("campaign: saved {}", path.display()),
            Err(error) => {
                println!("campaign: save {stem} failed ({error})");
                if let AppScreen::Shell(shell) = &mut self.screen {
                    shell.show_save_error(stem);
                }
            }
        }
    }

    fn return_to_main_menu(&mut self) {
        self.campaign_flow.run = None;
        if let AppScreen::Shell(shell) = &mut self.screen {
            shell.campaign_to_main_menu();
        }
    }

    fn run_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::StartMission {
                index,
                play_intro_movie,
                ..
            } => {
                let plan = self
                    .campaign_flow
                    .run
                    .as_ref()
                    .and_then(|run| run.plan(index, play_intro_movie));
                let Some(plan) = plan else {
                    println!("campaign: mission {index} has no map - back to the menu");
                    self.return_to_main_menu();
                    return;
                };
                println!(
                    "campaign: mission {index} on {} (intro movie {:?}, fade-up {} frames)",
                    plan.config.map,
                    plan.presentation.intro_movie,
                    plan.presentation.fade_up_frames
                );
                self.campaign_flow.presentation = plan.presentation.clone();
                if let AppScreen::Shell(shell) = &mut self.screen {
                    shell.start_campaign_mission(&plan);
                }
            }
            Outcome::MainMenu => {
                println!("campaign: over - back to the main menu");
                self.return_to_main_menu();
            }
        }
    }

    fn campaign_action(&mut self, action: CampaignAction) {
        let Some(run) = &mut self.campaign_flow.run else {
            match action {
                CampaignAction::TimeLineContinue => {
                    println!("campaign: no campaign run - CampaignMenu pushed as retail does");
                    if let AppScreen::Shell(shell) = &mut self.screen {
                        shell.show_campaign_menu(CampaignMenuData::default());
                    }
                }
                _ => self.return_to_main_menu(),
            }
            return;
        };
        match action {
            CampaignAction::NextMission => {
                let outcome = run.state.next_mission();
                self.run_outcome(outcome);
            }
            CampaignAction::LastMission => {
                let outcome = run.state.last_mission();
                self.run_outcome(outcome);
            }
            CampaignAction::MainMenu => self.return_to_main_menu(),
            CampaignAction::TimeLineContinue => match run.state.close_time_line() {
                TimeLineClose::CampaignComplete => {
                    if run.unlocks_bonus {
                        match crate::user_options::set_bonus_unlocked() {
                            Ok(path) => {
                                println!("campaign: BCU = true written to {}", path.display());
                            }
                            Err(error) => {
                                println!("campaign: could not write Options.ini ({error})");
                            }
                        }
                    }
                    self.return_to_main_menu();
                }
                TimeLineClose::CampaignMenu { autosave } => {
                    if autosave {
                        match crate::campaign_save::write_autosave(
                            &self.vfs,
                            &self.install_roots,
                            run,
                        ) {
                            Ok(path) => {
                                println!("campaign: autosave written to {}", path.display())
                            }
                            Err(error) => println!("campaign: autosave not written ({error})"),
                        }
                    }
                    let index = run.state.menu_mission_index();
                    let mission = run.mission(index);
                    let data = CampaignMenuData {
                        mission_name: mission.map(|m| m.name.clone()).unwrap_or_default(),
                        load_image: mission.and_then(|m| m.load_screen_image.clone()),
                        victorious: run.state.victorious(),
                    };
                    println!(
                        "campaign: CampaignMenu - next mission {:?}",
                        data.mission_name
                    );
                    if let AppScreen::Shell(shell) = &mut self.screen {
                        shell.show_campaign_menu(data);
                    }
                }
            },
        }
    }

    pub(crate) fn campaign_match_ended(
        &mut self,
        end: Option<MissionEnd>,
        carried: Option<mint_sage_campaign::Carryover>,
    ) {
        let Some(run) = &mut self.campaign_flow.run else {
            return;
        };
        match end {
            Some(MissionEnd::Victory) => {
                if let Some(carried) = carried {
                    run.carry = carried;
                }
                run.state.script_action("VICTORY");
            }
            Some(MissionEnd::Defeat) => {
                run.state.script_action("DEFEAT");
            }
            None => {}
        }
        run.state.match_torn_down();
    }

    pub(crate) fn campaign_running(&self) -> bool {
        self.campaign_flow.run.is_some()
    }
}
