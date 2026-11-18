use std::sync::Arc;
use std::time::Duration;

use mint_sage_apt_player::{ImageKey, MovieEvent, Value};
use mint_sage_apt_viewer::gadget::gadget_key;
use mint_sage_apt_viewer::host::{Change, GadgetClick, GadgetEvent};
use mint_vfs::Vfs;

use crate::apt_overlay::{Hosted, PLAY_SOUND};
use crate::options::{self, Gadget, GadgetValue, OptionsPreferences};
use crate::save_load::{Exit, Mode, SaveLoad};

const OPTIONS_INIT: &str = "AptOptions::InitGadgets";

pub(crate) const QUIT_MENU_MOVIE: &str = "quitmenu";
pub(crate) const OPTIONS_MOVIE: &str = "options";
pub(crate) const MENU_READY: &str = "OnInitialized";

const UNBUILT_BUTTONS: [&str; 1] = ["Save"];
const LOAD_BUTTON: &str = "Load";
const CAMPAIGN_TAB: u8 = 1;

const RESTART_LABEL: &str = "RestartOrSurrender";
pub(crate) const RESTART_TEXT: &str = "APT:Restart";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MenuRequest {
    ExitMission,
    RestartMission,
    ApplyOptions(OptionsPreferences),
    LoadSave(std::path::PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    Quit,
    Options,
    SaveLoad,
}

pub(crate) struct InGameMenus {
    quit: Option<Hosted>,
    options: Option<Hosted>,
    save_load: Option<Hosted>,
    save_screen: Option<SaveLoad>,
    save_sources: Option<(Arc<Vfs>, Arc<mint_sage_str::StringTable>)>,
    load_enabled: bool,
    shown: Option<Shown>,
    requests: Vec<MenuRequest>,
    restart_label: Option<String>,
    options_preferences: OptionsPreferences,
    options_draft: Option<OptionsPreferences>,
    options_bound: Vec<String>,
    detail_labels: [String; 6],
    options_gadget_held: bool,
    last_pointer: [f32; 2],
}

impl InGameMenus {
    pub(crate) fn new(
        quit: Option<Hosted>,
        options: Option<Hosted>,
        restart_label: Option<String>,
        strings: &mint_sage_str::StringTable,
    ) -> Self {
        let detail_labels = options::DETAIL_ENTRIES.map(|(key, _)| {
            let found = mint_sage_apt_viewer::localise(&format!("${key}"), strings);
            if found.starts_with("MISSING:") {
                key.to_owned()
            } else {
                found
            }
        });
        Self {
            quit,
            options,
            save_load: None,
            save_screen: None,
            save_sources: None,
            load_enabled: false,
            shown: None,
            requests: Vec::new(),
            restart_label,
            options_preferences: OptionsPreferences::default(),
            options_draft: None,
            options_bound: Vec::new(),
            options_gadget_held: false,
            last_pointer: [0.0, 0.0],
            detail_labels,
        }
    }

    pub(crate) fn with_save_load(
        mut self,
        hosted: Option<Hosted>,
        vfs: Arc<Vfs>,
        strings: Arc<mint_sage_str::StringTable>,
    ) -> Self {
        self.save_load = hosted;
        self.save_sources = Some((vfs, strings));
        self
    }

    pub(crate) fn set_load_enabled(&mut self, enabled: bool) {
        self.load_enabled = enabled;
    }

    pub(crate) fn set_options_defaults(&mut self, defaults: OptionsPreferences) {
        self.options_preferences = defaults;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.shown.is_some()
    }

    pub(crate) fn quit_shown(&self) -> bool {
        self.shown == Some(Shown::Quit)
    }

    pub(crate) fn options_shown(&self) -> bool {
        self.shown == Some(Shown::Options)
    }

    pub(crate) fn toggle(&mut self) {
        match self.shown {
            None => self.open_quit(),
            Some(Shown::Quit) => {
                println!("quit menu: closed (toggled)");
                self.shown = None;
            }
            Some(Shown::Options) => println!("quit menu: Options is up - toggle ignored"),
            Some(Shown::SaveLoad) => {
                if let Some(screen) = self.save_screen.as_mut() {
                    screen.escape();
                }
            }
        }
    }

    fn open_quit(&mut self) {
        let Some(quit) = self.quit.as_mut() else {
            println!("quit menu: the QuitMenu movie did not load - nothing to open");
            return;
        };
        if let Err(error) = quit.restart() {
            println!("quit menu: did not open - {error}");
            return;
        }
        quit.expose.set_extern("InGame", Value::Bool(true));
        quit.expose
            .set_extern("AptQuitMenu::RestartPopupType", Value::str("Restart"));
        quit.expose
            .string(RESTART_LABEL, self.restart_label.clone());
        println!("quit menu: open - the match is paused");
        self.shown = Some(Shown::Quit);
    }

    fn open_save_load(&mut self) {
        self.shown = None;
        let env = self
            .save_sources
            .as_ref()
            .and_then(|(vfs, strings)| crate::save_load::env(vfs, strings));
        let (Some(hosted), Some(env)) = (self.save_load.as_mut(), env) else {
            println!("load: the load screen is not available - back to the match");
            return;
        };
        if let Err(error) = hosted.restart() {
            println!("load: did not open - {error}");
            return;
        }
        hosted.expose.set_extern("InGame", Value::Bool(true));
        let screen = SaveLoad::open(Mode::Load, CAMPAIGN_TAB, true, env);
        hosted.view.player.set_sync_host(Some(screen.sync_host()));
        self.save_screen = Some(screen);
        self.options_gadget_held = false;
        println!("load: open");
        self.shown = Some(Shown::SaveLoad);
    }

    fn save_load_frame(&mut self, dt: Duration, sounds: &mut Vec<MovieEvent>) {
        let (Some(hosted), Some(screen)) = (self.save_load.as_mut(), self.save_screen.as_mut())
        else {
            self.shown = None;
            return;
        };
        let frame = screen.frame(hosted, dt);
        sounds.extend(
            frame
                .others
                .into_iter()
                .filter(|event| event.command() == Some(PLAY_SOUND)),
        );
        let Some(exit) = frame.exit else {
            return;
        };
        hosted.view.player.set_sync_host(None);
        self.save_screen = None;
        self.shown = None;
        match exit {
            Exit::Load(path) => {
                println!("load: {} - leaving the match", path.display());
                self.requests.push(MenuRequest::LoadSave(path));
            }
            Exit::Closed | Exit::Save { .. } => self.open_quit(),
        }
    }

    fn open_options(&mut self) {
        self.shown = None;
        let Some(options) = self.options.as_mut() else {
            println!("options: the Options movie did not load - back to the match");
            return;
        };
        if let Err(error) = options.restart() {
            println!("options: did not open - {error}");
            return;
        }
        options.expose.set_extern("InGame", Value::Bool(true));
        self.options_draft = Some(self.options_preferences);
        println!("options: open");
        self.shown = Some(Shown::Options);
    }

    fn declare_options(&mut self) {
        let committed = self.options_preferences;
        let draft = *self.options_draft.get_or_insert(committed);
        let Some(options) = self.options.as_mut() else {
            return;
        };
        for (name, gadget) in options::ALL {
            options.expose.image(
                ImageKey::Name(gadget_key(OPTIONS_INIT, name)),
                Some(options::image_source(
                    draft.value(gadget),
                    &self.detail_labels,
                )),
            );
        }
    }

    fn gadget_changed(&mut self, event: &GadgetEvent) {
        let Some(key) = event.key.as_deref() else {
            return;
        };
        if self.shown == Some(Shown::SaveLoad) {
            if let Some(screen) = self.save_screen.as_mut() {
                screen.gadget_changed(key, event.change);
            }
            return;
        }
        let Some((name, gadget)) = options::ALL
            .into_iter()
            .find(|(name, _)| gadget_key(OPTIONS_INIT, name) == key)
        else {
            return;
        };
        let Some(draft) = self.options_draft.as_mut() else {
            return;
        };
        let value = match (event.change, draft.value(gadget)) {
            (Change::Value(v), GadgetValue::Slider(_)) => {
                GadgetValue::Slider(v.max(0).unsigned_abs())
            }
            (Change::Checked(c), GadgetValue::Check(_)) => GadgetValue::Check(c),
            (Change::Row(row), GadgetValue::Combo(_)) => {
                let Some(&(_, level)) = options::DETAIL_ENTRIES.get(row) else {
                    return;
                };
                GadgetValue::Combo(level)
            }
            _ => return,
        };
        draft.set(gadget, value);
        if !self.options_bound.iter().any(|bound| bound == name) {
            self.options_bound.push(name.to_owned());
        }
        println!("options: {name} - {value:?}");
        if gadget == Gadget::Detail && value == GadgetValue::Combo(5) {
            if let Some(options) = self.options.as_mut() {
                options.expose.fire("ShowAdvancedSettings", vec![]);
            }
        }
    }

    pub(crate) fn pointer_moved(&mut self, position: [f32; 2]) {
        self.last_pointer = position;
        let held = self.options_gadget_held;
        let Some(shown) = self.shown else {
            return;
        };
        let Some(hosted) = self.shown_mut() else {
            return;
        };
        let gadget_event = (shown != Shown::Quit)
            .then(|| hosted.view.gadget_pointer_moved(position, held))
            .flatten();
        let stage = hosted.view.resolve_stage_point(position);
        let _ = hosted.view.player.pointer_move(stage[0], stage[1]);
        if let Some(event) = gadget_event {
            self.gadget_changed(&event);
        }
    }

    pub(crate) fn pointer_button(&mut self, pressed: bool) {
        let position = self.last_pointer;
        let Some(shown) = self.shown else {
            return;
        };
        if shown != Shown::Quit {
            if !pressed && self.options_gadget_held {
                self.options_gadget_held = false;
                let click = self
                    .shown_mut()
                    .map(|hosted| hosted.view.gadget_released(position));
                if let Some(GadgetClick::Changed(event)) = click {
                    self.gadget_changed(&event);
                }
                return;
            }
            let modal =
                shown == Shown::SaveLoad && self.save_screen.as_ref().is_some_and(SaveLoad::modal);
            let pressed_on_gadget = pressed
                && !modal
                && self
                    .shown_mut()
                    .is_some_and(|hosted| hosted.view.gadget_pressed(position));
            if pressed_on_gadget {
                self.options_gadget_held = true;
                return;
            }
        }
        let Some(hosted) = self.shown_mut() else {
            return;
        };
        let _ = hosted.view.player.pointer_button(pressed);
    }

    pub(crate) fn shown_mut(&mut self) -> Option<&mut Hosted> {
        match self.shown? {
            Shown::Quit => self.quit.as_mut(),
            Shown::Options => self.options.as_mut(),
            Shown::SaveLoad => self.save_load.as_mut(),
        }
    }

    pub(crate) fn shown(&self) -> Option<&Hosted> {
        match self.shown? {
            Shown::Quit => self.quit.as_ref(),
            Shown::Options => self.options.as_ref(),
            Shown::SaveLoad => self.save_load.as_ref(),
        }
    }

    pub(crate) fn all_mut(&mut self) -> impl Iterator<Item = &mut Hosted> {
        self.quit
            .iter_mut()
            .chain(self.options.iter_mut())
            .chain(self.save_load.iter_mut())
    }

    pub(crate) fn frame(&mut self, dt: Duration, sounds: &mut Vec<MovieEvent>) {
        let Some(shown) = self.shown else {
            return;
        };
        if shown == Shown::SaveLoad {
            self.save_load_frame(dt, sounds);
            return;
        }
        if shown == Shown::Options {
            self.declare_options();
        }
        let Some(hosted) = self.shown_mut() else {
            self.shown = None;
            return;
        };
        hosted.frame(dt);
        let events = hosted.expose.take_events();
        for event in events {
            if event.command() == Some(PLAY_SOUND) {
                sounds.push(event);
                continue;
            }
            let Some(command) = event.command() else {
                continue;
            };
            match shown {
                Shown::Quit => self.on_quit_call(command),
                Shown::Options => self.on_options_call(command, event.args().unwrap_or("")),
                Shown::SaveLoad => {}
            }
        }
    }

    fn on_quit_call(&mut self, command: &str) {
        match command {
            "OnInitialized" => {
                if let Some(quit) = self.quit.as_mut() {
                    let load =
                        (!(self.load_enabled && self.save_load.is_some())).then_some(LOAD_BUTTON);
                    for button in UNBUILT_BUTTONS.into_iter().chain(load) {
                        quit.expose.fire("disableButton", vec![Value::str(button)]);
                    }
                }
            }
            "ReturnToGame" => {
                println!("quit menu: Resume - back to the match");
                self.shown = None;
            }
            "ExitMission" => {
                println!("quit menu: Exit Mission confirmed - leaving for the shell");
                self.shown = None;
                self.requests.push(MenuRequest::ExitMission);
            }
            "RestartMission" => {
                println!("quit menu: Restart confirmed - the same match again");
                self.shown = None;
                self.requests.push(MenuRequest::RestartMission);
            }
            "OptionsScreen" => self.open_options(),
            "LoadMenu" => self.open_save_load(),
            other => println!("quit menu: AptQuitMenu::{other} - not answered"),
        }
    }

    fn on_options_call(&mut self, command: &str, args: &str) {
        match command {
            "OnInitialized" => {}
            "InitGadgets" => {
                if self.init_gadget(args).is_none() {
                    println!("options: InitGadgets({args}) - not one of the engine's own gadgets");
                }
            }
            "Cancel" => {
                self.options_draft = None;
                println!("options: Cancel - back to the quit menu");
                self.shown = None;
                self.open_quit();
            }
            "Save" => {
                if let Some(draft) = self.options_draft.take() {
                    if draft != self.options_preferences {
                        self.options_preferences = draft;
                        self.requests.push(MenuRequest::ApplyOptions(draft));
                    }
                }
                println!(
                    "options: Save - {} gadget(s) bound, back to the quit menu",
                    self.options_bound.len()
                );
                self.shown = None;
                self.open_quit();
            }
            other => println!("options: AptOptions::{other} - not answered"),
        }
    }

    pub(crate) fn init_gadget(&mut self, name: &str) -> Option<GadgetValue> {
        let gadget = Gadget::named(name)?;
        if !self.options_bound.iter().any(|bound| bound == name) {
            self.options_bound.push(name.to_owned());
        }
        let committed = self.options_preferences;
        let draft = self.options_draft.get_or_insert(committed);
        Some(draft.value(gadget))
    }

    pub(crate) fn take_requests(&mut self) -> Vec<MenuRequest> {
        std::mem::take(&mut self.requests)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use mint_render::{Error as RenderError, Renderer};
    use mint_sage_apt_player::synth::{Frames, Synth};
    use mint_sage_apt_player::{Library, Limits};
    use mint_sage_apt_viewer::host::MovieView;

    use super::*;

    fn quit_menu_up() -> InGameMenus {
        let mut menus = InGameMenus::new(None, None, None, &mint_sage_str::StringTable::new());
        menus.shown = Some(Shown::Quit);
        menus
    }

    fn test_renderer() -> Option<Renderer> {
        match Renderer::offscreen(4, 4) {
            Ok(renderer) => Some(renderer),
            Err(RenderError::NoAdapter) => {
                eprintln!("SKIPPED: no graphics adapter on this machine");
                None
            }
            Err(error) => panic!("offscreen renderer: {error}"),
        }
    }

    #[allow(clippy::expect_used)]
    fn synthetic_quit(renderer: &mut Renderer) -> Hosted {
        let synth = Synth::new();
        let root: Frames = vec![vec![]];
        let (constants, apt) = synth.build(&root);
        let mut library = Library::new();
        library
            .add("test", &constants, apt)
            .expect("synthetic movie parses");
        library.link();
        let view = MovieView::load(
            Arc::new(library),
            Vec::new(),
            "test",
            Arc::new(HashMap::new()),
            Arc::new(mint_sage_str::StringTable::new()),
            (800.0, 600.0),
            Limits::default(),
            renderer,
        )
        .expect("synthetic movie view loads");
        Hosted::new(view, QUIT_MENU_MOVIE, MENU_READY)
    }

    #[test]
    fn exit_and_restart_close_the_menu_and_ask_the_match_for_their_own_request() {
        for (call, request) in [
            ("ExitMission", MenuRequest::ExitMission),
            ("RestartMission", MenuRequest::RestartMission),
        ] {
            let mut menus = quit_menu_up();
            menus.on_quit_call(call);
            assert!(!menus.is_open(), "{call}");
            assert_eq!(menus.take_requests(), vec![request], "{call}");
        }
    }

    #[test]
    fn resume_and_a_second_toggle_close_without_a_request() {
        let mut menus = quit_menu_up();
        menus.on_quit_call("ReturnToGame");
        assert!(!menus.is_open());
        let mut menus = quit_menu_up();
        menus.toggle();
        assert!(!menus.is_open());
        assert!(menus.take_requests().is_empty());
    }

    #[test]
    fn escape_while_options_is_up_does_not_toggle_the_quit_menu() {
        let mut menus = InGameMenus::new(None, None, None, &mint_sage_str::StringTable::new());
        menus.shown = Some(Shown::Options);
        menus.toggle();
        assert_eq!(menus.shown, Some(Shown::Options), "Options must stay up");
    }

    #[test]
    fn a_menu_whose_movie_did_not_load_never_opens() {
        let mut menus = InGameMenus::new(None, None, None, &mint_sage_str::StringTable::new());
        menus.toggle();
        assert!(!menus.is_open());
        let mut menus = quit_menu_up();
        menus.on_quit_call("OptionsScreen");
        assert!(!menus.is_open());
    }

    fn options_up() -> InGameMenus {
        let mut menus = InGameMenus::new(None, None, None, &mint_sage_str::StringTable::new());
        menus.shown = Some(Shown::Options);
        menus.options_draft = Some(menus.options_preferences);
        menus
    }

    #[test]
    fn init_gadgets_answers_with_the_stored_value_and_keeps_the_binding() {
        let mut menus = options_up();
        assert_eq!(
            menus.init_gadget("Options::MusicVolume"),
            Some(GadgetValue::Slider(100))
        );
        assert_eq!(menus.options_bound, vec!["Options::MusicVolume".to_owned()]);
        assert_eq!(menus.init_gadget("Options::NotOurs"), None);
    }

    #[test]
    fn save_commits_the_draft_and_cancel_discards_it() {
        let mut menus = options_up();
        menus.options_draft.as_mut().unwrap().sound_fx_volume = 40;
        menus.on_options_call("Save", "");
        assert_eq!(menus.options_preferences.sound_fx_volume, 40);
        assert_eq!(
            menus.take_requests(),
            vec![MenuRequest::ApplyOptions(menus.options_preferences)]
        );

        let mut menus = options_up();
        menus.options_draft.as_mut().unwrap().sound_fx_volume = 40;
        menus.on_options_call("Cancel", "");
        assert_eq!(
            menus.options_preferences.sound_fx_volume, 100,
            "Cancel must not commit the draft"
        );
        assert!(menus.take_requests().is_empty());
    }

    #[test]
    fn save_and_cancel_return_to_the_quit_menu() {
        let Some(mut renderer) = test_renderer() else {
            return;
        };
        for command in ["Save", "Cancel"] {
            let mut menus = InGameMenus::new(
                Some(synthetic_quit(&mut renderer)),
                None,
                None,
                &mint_sage_str::StringTable::new(),
            );
            menus.shown = Some(Shown::Options);
            menus.options_draft = Some(menus.options_preferences);
            menus.on_options_call(command, "");
            assert_eq!(
                menus.shown,
                Some(Shown::Quit),
                "{command} must reopen QuitMenu"
            );
        }
    }
}
