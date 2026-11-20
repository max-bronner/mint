use std::time::Duration;

use mint_app::{Key, PointerButton};
use mint_render::Renderer;
use mint_sage_apt_player::Value;
use mint_sage_apt_viewer::host::{GadgetClick, GadgetEvent, MovieView};
use mint_sage_apt_viewer::{ScaleMode, TextAnchor};

use super::message_box::{BoxKind, BoxRequest};
use super::{ShellApp, IN_ENGINE};
use crate::apt_overlay::Hosted;
use crate::save_load::{self, Exit, Mode, SaveLoad};

#[derive(Default)]
pub(super) struct Saves {
    hosted: Option<Hosted>,
    screen: Option<SaveLoad>,
    exit: Option<Exit>,
    held: bool,
}

impl Saves {
    pub(super) fn load(
        load_one: &impl Fn(&str, &mut Renderer) -> Result<MovieView, String>,
        renderer: &mut Renderer,
        scale_mode: ScaleMode,
    ) -> Self {
        let hosted = match load_one(save_load::MOVIE, renderer) {
            Ok(view) => {
                let mut hosted = Hosted::new(view, save_load::MOVIE, save_load::READY);
                hosted.view.scale_mode = scale_mode;
                Some(hosted)
            }
            Err(error) => {
                println!("shell: the load/save screen will not open - {error}");
                None
            }
        };
        Self {
            hosted,
            ..Self::default()
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.screen.is_some()
    }

    pub(super) fn set_scale_mode(&mut self, mode: ScaleMode) {
        if let Some(hosted) = self.hosted.as_mut() {
            hosted.view.scale_mode = mode;
        }
    }

    pub(super) fn set_text_anchor(&mut self, anchor: TextAnchor) {
        if let Some(hosted) = self.hosted.as_mut() {
            hosted.view.text_anchor = anchor;
        }
    }

    pub(super) fn set_window_size(&mut self, size: (f32, f32)) {
        if let Some(hosted) = self.hosted.as_mut() {
            hosted.view.window_size = size;
        }
    }

    pub(super) fn shown_mut(&mut self) -> Option<&mut Hosted> {
        self.hosted.as_mut().filter(|_| self.screen.is_some())
    }

    pub(super) fn shown(&self) -> Option<&Hosted> {
        self.hosted.as_ref().filter(|_| self.screen.is_some())
    }
}

impl ShellApp {
    pub(super) fn open_save_load(&mut self, mode: Mode, mask: u8) {
        if self.saves.screen.is_some() {
            return;
        }
        let Some(env) = save_load::env(&self.vfs, &self.strings) else {
            println!("shell: load/save: no user data folder");
            return;
        };
        let Some(hosted) = self.saves.hosted.as_mut() else {
            println!("shell: load/save: the SaveLoad movie did not load");
            return;
        };
        if let Err(error) = hosted.restart() {
            println!("shell: load/save: did not open - {error}");
            return;
        }
        hosted.expose.set_extern(IN_ENGINE, Value::Bool(true));
        let screen = SaveLoad::open(mode, mask, false, env);
        hosted.view.player.set_sync_host(Some(screen.sync_host()));
        println!(
            "shell: load/save open - {mode:?}, types {}",
            save_load::game_types(mask)
        );
        self.saves.screen = Some(screen);
        self.saves.held = false;
    }

    pub(super) fn save_load_frame(&mut self, elapsed: Duration) {
        let (Some(hosted), Some(screen)) = (self.saves.hosted.as_mut(), self.saves.screen.as_mut())
        else {
            return;
        };
        let frame = screen.frame(hosted, elapsed);
        for event in frame.others {
            self.played(&event);
        }
        if let Some(exit) = frame.exit {
            self.saves.screen = None;
            if let Some(hosted) = self.saves.hosted.as_mut() {
                hosted.view.player.set_sync_host(None);
            }
            println!("shell: load/save closed - {exit:?}");
            if exit != Exit::Closed {
                self.saves.exit = Some(exit);
            }
        }
    }

    pub(crate) fn take_save_exit(&mut self) -> Option<Exit> {
        self.saves.exit.take()
    }

    pub(super) fn save_load_pointer_moved(&mut self, x: f32, y: f32) {
        self.pointer = (x, y);
        let held = self.saves.held;
        let Some(hosted) = self.saves.hosted.as_mut() else {
            return;
        };
        let event = hosted.view.gadget_pointer_moved([x, y], held);
        let stage = hosted.view.resolve_stage_point([x, y]);
        let _ = hosted.view.player.pointer_move(stage[0], stage[1]);
        self.save_load_gadget(event);
    }

    pub(super) fn save_load_pointer_button(&mut self, button: PointerButton, pressed: bool) {
        if button != PointerButton::Primary {
            return;
        }
        let at = [self.pointer.0, self.pointer.1];
        let Some(hosted) = self.saves.hosted.as_mut() else {
            return;
        };
        if !pressed && self.saves.held {
            self.saves.held = false;
            let click = hosted.view.gadget_released(at);
            if let GadgetClick::Changed(event) = click {
                self.save_load_gadget(Some(event));
            }
            return;
        }
        let modal = self.saves.screen.as_ref().is_some_and(SaveLoad::modal);
        if pressed && !modal && hosted.view.gadget_pressed(at) {
            self.saves.held = true;
            return;
        }
        let stage = hosted.view.resolve_stage_point(at);
        let _ = hosted.view.player.pointer_move(stage[0], stage[1]);
        let _ = hosted.view.player.pointer_button(pressed);
    }

    fn save_load_gadget(&mut self, event: Option<GadgetEvent>) {
        if let (Some(event), Some(screen)) = (event, self.saves.screen.as_mut()) {
            if let Some(key) = event.key.as_deref() {
                screen.gadget_changed(key, event.change);
            }
        }
    }

    pub(super) fn save_load_scroll(&mut self, lines: f32) {
        let at = [self.pointer.0, self.pointer.1];
        if let Some(hosted) = self.saves.hosted.as_mut() {
            let _ = hosted.view.list_scrolled(at, lines);
        }
    }

    pub(super) fn save_load_key(&mut self, key: Key, pressed: bool) {
        if let Some(screen) = self.saves.screen.as_mut() {
            screen.key(key, pressed);
        }
    }

    pub(crate) fn show_load_error(&mut self, file: &str) {
        self.save_error_box("GUI:ErrorLoadingGame", file);
    }

    pub(crate) fn show_save_error(&mut self, file: &str) {
        self.save_error_box("GUI:ErrorSavingGame", file);
    }

    fn save_error_box(&mut self, label: &str, file: &str) {
        let text = self.strings.get(label).map_or_else(
            || label.to_owned(),
            |t| mint_sage_str::format_placeholder(t, mint_sage_str::Placeholder::Str(file)),
        );
        let title = self
            .strings
            .get("GUI:Error")
            .map_or_else(|| "GUI:Error".to_owned(), str::to_owned);
        self.show_message_box(&BoxRequest {
            kind: BoxKind::Ok,
            title,
            text,
        });
    }
}
