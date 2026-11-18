use std::sync::Arc;
use std::time::Duration;

use mint_sage_apt_player::expose::resolve_path;
use mint_sage_apt_player::{ImageKey, MovieEvent, Value};

use crate::apt_overlay::{Hosted, PLAY_SOUND};
use crate::help_box::HelpLineSpec;
use crate::hud::ImageSource;
use crate::notification_box::Entry;

const TITLE: &str = "_level0_Title";
const ICON: &str = "_level0_Icon";
const MESSAGE: &str = "_level0_Message";
const MESSAGE_WIDTH: &str = "_level0_MessageWidth";
const CONTENT: &str = "box.content";

pub(crate) const READY: &str = "_level0_OnInitialized";
const OPENED: &str = "_level0_OnOpen";
const CLOSED: &str = "_level0_OnClosed";
const CLOSE_CLICKED: &str = "_level0_OnCloseButtonClicked";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Callback {
    Initialized,
    Opened,
    Closed,
    CloseClicked,
}

pub(crate) struct NotificationHost {
    hosted: Hosted,
    icon_visible: Option<bool>,
    active: bool,
}

impl NotificationHost {
    pub(crate) fn new(mut hosted: Hosted) -> Self {
        hosted.expose.set_extern("InGame", Value::Bool(true));
        Self {
            hosted,
            icon_visible: None,
            active: false,
        }
    }

    pub(crate) fn hosted(&self) -> &Hosted {
        &self.hosted
    }

    pub(crate) fn hosted_mut(&mut self) -> &mut Hosted {
        &mut self.hosted
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    pub(crate) fn message_width(&self) -> f32 {
        match self.hosted.view.player.extern_member(MESSAGE_WIDTH) {
            Some(Value::Number(width)) if width.is_finite() && width > 0.0 => {
                #[allow(clippy::cast_possible_truncation)]
                let width = width as f32;
                width
            }
            _ => 0.0,
        }
    }

    pub(crate) fn frame(&mut self, dt: Duration) -> (Vec<Callback>, Vec<MovieEvent>) {
        self.hosted.frame(dt);
        let (mut callbacks, mut sounds) = (Vec::new(), Vec::new());
        for event in self.hosted.expose.take_events() {
            match event.command() {
                Some(READY) => callbacks.push(Callback::Initialized),
                Some(OPENED) => callbacks.push(Callback::Opened),
                Some(CLOSED) => {
                    self.active = false;
                    callbacks.push(Callback::Closed);
                }
                Some(CLOSE_CLICKED) => callbacks.push(Callback::CloseClicked),
                Some(PLAY_SOUND) => sounds.push(event),
                _ => {}
            }
        }
        (callbacks, sounds)
    }

    pub(crate) fn present(&mut self, entry: &Entry) {
        let expose = &mut self.hosted.expose;
        expose.string(TITLE, Some(entry.title.clone()));
        let icon_visible = entry.icon.is_some();
        if self.icon_visible != Some(icon_visible) {
            self.icon_visible = Some(icon_visible);
            expose.fire("SetIconVisibility", vec![Value::Bool(icon_visible)]);
        }
        expose.image(
            ImageKey::Name(ICON.to_owned()),
            entry.icon.clone().map(ImageSource::MappedImage),
        );
        let lines = message_lines(entry);
        let drawn: Vec<_> = lines.iter().map(HelpLineSpec::to_text_line).collect();
        let at = resolve_path(&self.hosted.view.player, CONTENT);
        let width = at
            .and_then(|id| self.hosted.view.stage_rect(id))
            .map(|[l, _, r, _]| r - l)
            .filter(|width| *width > 1.0)
            .unwrap_or_else(|| self.message_width());
        let height = self.hosted.view.measure_text_box_height(&drawn, width, at);
        self.hosted.view.warm_text_box(&drawn, width, at);
        let expose = &mut self.hosted.expose;
        expose.image(
            ImageKey::Name(MESSAGE.to_owned()),
            Some(ImageSource::Message(Arc::new(lines))),
        );
        let opaque = entry.anchor != crate::notification_box::Anchor::Main;
        expose.fire(
            "Open",
            vec![
                Value::Number(f64::from(height)),
                Value::Bool(opaque),
                Value::str(entry.anchor.name()),
            ],
        );
        self.active = true;
    }

    pub(crate) fn close(&mut self) {
        self.hosted.expose.fire("Close", Vec::new());
    }

    pub(crate) fn hide(&mut self) {
        self.hosted.expose.fire("Hide", Vec::new());
        self.active = false;
    }
}

fn message_lines(entry: &Entry) -> Vec<HelpLineSpec> {
    let Some(font) = entry.font.as_ref() else {
        return Vec::new();
    };
    let [r, g, b, _] = entry.colour.unwrap_or([255, 255, 255, 255]);
    vec![HelpLineSpec {
        text: entry.message.clone(),
        font: font.name.clone(),
        point_size_bits: font.size.to_bits(),
        bold: font.bold,
        color: [r, g, b],
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification_box::Anchor;

    #[test]
    fn the_message_surface_takes_the_entrys_font_and_colour() {
        let entry = Entry {
            title: String::new(),
            icon: None,
            open_audio: None,
            message: "hello".to_owned(),
            font: Some(mint_sage_ini::MessageFont {
                name: "Face".to_owned(),
                size: 16.0,
                bold: false,
            }),
            colour: Some([255, 204, 0, 255]),
            ms: 0,
            anchor: Anchor::Main,
        };
        let lines = message_lines(&entry);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            (
                lines[0].font.as_str(),
                lines[0].color,
                lines[0].point_size()
            ),
            ("Face", [255, 204, 0], 16.0)
        );
    }
}
