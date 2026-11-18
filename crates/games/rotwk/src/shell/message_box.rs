use std::time::Duration;

use mint_render::Renderer;
use mint_sage_apt_player::Value;
use mint_sage_apt_viewer::host::MovieView;
use mint_sage_apt_viewer::{ScaleMode, TextAnchor};

use super::ShellApp;
use crate::apt_overlay::{Hosted, GUI_FX_MOVIE, GUI_FX_READY};

const CLIP: &str = "MessageBox";
const TEXT_VARIABLE_ROOT: &str = "_level0";
const LARGE_ABOVE_CHARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum BoxKind {
    Ok,
    OkCancel,
    YesNo,
    Cancel,
    NonInteractive,
}

impl BoxKind {
    fn movie_name(self) -> &'static str {
        match self {
            Self::Ok => "Ok",
            Self::OkCancel => "OkCancel",
            Self::YesNo => "YesNo",
            Self::Cancel => "Cancel",
            Self::NonInteractive => "NonInteractive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoxRequest {
    pub(crate) kind: BoxKind,
    pub(crate) title: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoxButton {
    Yes,
    No,
    Ok,
    Cancel,
}

impl BoxButton {
    pub(crate) fn from_command(command: &str) -> Option<Self> {
        let (clip, button) = command.rsplit_once('_')?;
        if !clip.ends_with(CLIP) {
            return None;
        }
        match button {
            "OnButtonYes" => Some(Self::Yes),
            "OnButtonNo" => Some(Self::No),
            "OnButtonOk" => Some(Self::Ok),
            "OnButtonCancel" => Some(Self::Cancel),
            _ => None,
        }
    }
}

fn is_large(text: &str) -> bool {
    text.chars().count() > LARGE_ABOVE_CHARS
}

pub(crate) struct MessageBoxHost {
    hosted: Option<Hosted>,
    up: bool,
}

impl MessageBoxHost {
    pub(crate) fn load(
        load_one: &impl Fn(&str, &mut Renderer) -> Result<MovieView, String>,
        renderer: &mut Renderer,
    ) -> Self {
        let hosted = match load_one(GUI_FX_MOVIE, renderer) {
            Ok(view) => Some(Hosted::new(view, GUI_FX_MOVIE, GUI_FX_READY)),
            Err(error) => {
                println!("shell: the message box will not show - {error}");
                None
            }
        };
        Self { hosted, up: false }
    }

    pub(crate) fn set_scale_mode(&mut self, mode: ScaleMode) {
        if let Some(hosted) = self.hosted.as_mut() {
            hosted.view.scale_mode = mode;
        }
    }

    pub(crate) fn set_text_anchor(&mut self, anchor: TextAnchor) {
        if let Some(hosted) = self.hosted.as_mut() {
            hosted.view.text_anchor = anchor;
        }
    }

    pub(crate) fn set_window_size(&mut self, size: (f32, f32)) {
        if let Some(hosted) = self.hosted.as_mut() {
            hosted.view.window_size = size;
        }
    }

    pub(crate) fn is_up(&self) -> bool {
        self.up
    }

    pub(crate) fn shown_mut(&mut self) -> Option<&mut Hosted> {
        self.hosted.as_mut().filter(|_| self.up)
    }

    pub(crate) fn shown(&self) -> Option<&Hosted> {
        self.hosted.as_ref().filter(|_| self.up)
    }

    pub(crate) fn show(&mut self, request: &BoxRequest) -> bool {
        let Some(hosted) = self.hosted.as_mut() else {
            return false;
        };
        let key = |part: &str| format!("{TEXT_VARIABLE_ROOT}.{CLIP}_{part}");
        hosted
            .expose
            .string(&key("Title"), Some(request.title.clone()));
        hosted
            .expose
            .string(&key("Text"), Some(request.text.clone()));
        hosted.expose.fire(
            &format!("{CLIP}.Show"),
            vec![
                Value::str(request.kind.movie_name()),
                Value::Bool(is_large(&request.text)),
            ],
        );
        self.up = true;
        true
    }

    fn hide(&mut self) {
        if let Some(hosted) = self.hosted.as_mut() {
            hosted
                .expose
                .fire(&format!("{CLIP}.Hide"), vec![Value::Bool(true)]);
        }
        self.up = false;
    }
}

impl ShellApp {
    pub(super) fn show_message_box(&mut self, request: &BoxRequest) {
        if !self.message_box.show(request) {
            println!(
                "shell: no message box - {}: {}",
                request.title, request.text
            );
        }
    }

    pub(super) fn message_box_frame(&mut self, elapsed: Duration) -> Option<BoxButton> {
        let hosted = self.message_box.shown_mut()?;
        hosted.frame(elapsed);
        let events = hosted.expose.take_events();
        let mut answer = None;
        for event in events {
            if let Some(button) = event.command().and_then(BoxButton::from_command) {
                answer.get_or_insert(button);
            } else {
                self.played(&event);
            }
        }
        if answer.is_some() {
            self.message_box.hide();
        }
        answer
    }

    pub(super) fn message_box_pointer_moved(&mut self, x: f32, y: f32) {
        self.pointer = (x, y);
        if let Some(hosted) = self.message_box.shown_mut() {
            let stage = hosted.view.resolve_stage_point([x, y]);
            let _ = hosted.view.player.pointer_move(stage[0], stage[1]);
        }
    }

    pub(super) fn message_box_pointer_button(&mut self, pressed: bool) {
        let at = [self.pointer.0, self.pointer.1];
        if let Some(hosted) = self.message_box.shown_mut() {
            let stage = hosted.view.resolve_stage_point(at);
            let _ = hosted.view.player.pointer_move(stage[0], stage[1]);
            let _ = hosted.view.player.pointer_button(pressed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_button_command_names_its_button_only_on_the_message_box_clip() {
        assert_eq!(
            BoxButton::from_command("_level0.MessageBox_OnButtonYes"),
            Some(BoxButton::Yes)
        );
        assert_eq!(
            BoxButton::from_command("_level13.MessageBox_OnButtonNo"),
            Some(BoxButton::No)
        );
        assert_eq!(BoxButton::from_command("_level0.Other_OnButtonYes"), None);
        assert_eq!(
            BoxButton::from_command("_level0.MessageBox_OnShowing"),
            None
        );
    }

    #[test]
    fn a_text_over_256_characters_uses_the_large_box() {
        assert!(!is_large(&"x".repeat(256)));
        assert!(is_large(&"x".repeat(257)));
    }
}
