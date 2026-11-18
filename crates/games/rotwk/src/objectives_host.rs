use std::time::Duration;

use mint_sage_apt_player::expose::resolve_path;
use mint_sage_apt_player::{MovieEvent, Value};

use crate::apt_overlay::{Hosted, PLAY_SOUND};

pub(crate) const MOVIE: &str = "objectives";
pub(crate) const READY: &str = "OnInitialized";
const RETURN_TO_GAME: &str = "ReturnToGame";
const INPUT_ENABLED: &str = "AptObjectivesMenu::InputEnabled";
const IN_SKIRMISH: &str = "InSkirmish";
const FRAME_CLIP: &str = "InnerFrame";
const CLOSE_PATIENCE: u32 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Feed {
    pub(crate) rows: Vec<(String, String)>,
    pub(crate) input_enabled: bool,
}

#[derive(Debug, Clone, Copy)]
struct Closing {
    seen_playing: bool,
    waited: u32,
}

pub(crate) struct ObjectivesHost {
    hosted: Hosted,
    open: bool,
    closing: Option<Closing>,
}

impl ObjectivesHost {
    pub(crate) fn new(hosted: Hosted) -> Self {
        Self {
            hosted,
            open: false,
            closing: None,
        }
    }

    pub(crate) fn hosted(&self) -> &Hosted {
        &self.hosted
    }

    pub(crate) fn hosted_mut(&mut self) -> &mut Hosted {
        &mut self.hosted
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn open(&mut self, feed: &Feed) -> Result<(), String> {
        self.hosted.restart()?;
        self.hosted.expose.set_extern("InGame", Value::Bool(true));
        self.hosted.expose.set_extern(IN_SKIRMISH, Value::str("0"));
        self.declare(feed);
        self.open = true;
        self.closing = None;
        Ok(())
    }

    fn declare(&mut self, feed: &Feed) {
        for (name, value) in &feed.rows {
            self.hosted.expose.set_extern(name, Value::str(value));
        }
        self.hosted.expose.set_extern(
            INPUT_ENABLED,
            Value::str(if feed.input_enabled { "1" } else { "0" }),
        );
    }

    pub(crate) fn escape(&mut self) {
        if self.closing.is_none() {
            self.hosted.expose.fire("EscapeKeyPressed", Vec::new());
            self.closing = Some(Closing {
                seen_playing: false,
                waited: 0,
            });
        }
    }

    fn close_finished(&mut self) -> bool {
        let Some(closing) = self.closing.as_mut() else {
            return false;
        };
        let player = &self.hosted.view.player;
        let playing = resolve_path(player, FRAME_CLIP)
            .and_then(|id| player.instance(id))
            .and_then(|instance| instance.clip())
            .is_some_and(|clip| clip.playing);
        closing.waited = closing.waited.saturating_add(1);
        closing.seen_playing |= playing;
        (closing.seen_playing && !playing) || closing.waited > CLOSE_PATIENCE
    }

    pub(crate) fn frame(&mut self, dt: Duration, feed: &Feed) -> Vec<MovieEvent> {
        self.declare(feed);
        self.hosted.frame(dt);
        let mut sounds = Vec::new();
        if self.close_finished() {
            self.open = false;
        }
        for event in self.hosted.expose.take_events() {
            match event.command() {
                Some(RETURN_TO_GAME) => self.open = false,
                Some(PLAY_SOUND) => sounds.push(event),
                _ => {}
            }
        }
        sounds
    }
}
