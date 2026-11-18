use std::time::Duration;

use mint_render::{Renderer, ScreenDraw};
use mint_sage_apt_player::Value;

use super::{AptOverlay, Hosted, TopTarget};
use crate::notification_box::{Anchor, Effect};
use crate::notification_host::Callback;
use crate::objectives_host::Feed;

const FLASH_ON: &str = "1";
const FLASH_OFF: &str = "0";
const EFFECT_SHOW: &str = "_show";
const EFFECT_HIDE: &str = "_hide";

impl AptOverlay {
    pub(crate) fn set_campaign(&mut self, campaign: bool) {
        self.campaign = campaign;
    }

    pub(crate) fn set_objectives_feed(&mut self, feed: Feed) {
        self.objectives_feed = feed;
    }

    pub(crate) fn note_new_objective(&mut self) {
        self.objectives_flash.new_objective = true;
    }

    pub(crate) fn set_timed_objectives_flash(&mut self, running: bool) {
        self.objectives_flash.timed = running;
    }

    pub(crate) fn objectives_open(&self) -> bool {
        self.objectives.as_ref().is_some_and(|o| o.is_open())
    }

    pub(crate) fn open_objectives(&mut self) {
        if self.script_ui.objectives_disabled {
            println!("objectives: a script disabled the screen - not opening");
            return;
        }
        let Some(screen) = self.objectives.as_mut() else {
            println!("objectives: the Objectives movie did not load - nothing to open");
            return;
        };
        if screen.is_open() {
            return;
        }
        match screen.open(&self.objectives_feed) {
            Ok(()) => println!(
                "objectives: open - {} shown row(s)",
                self.objectives_feed
                    .rows
                    .iter()
                    .filter(|(name, value)| !name.ends_with("Status") && !value.is_empty())
                    .count()
            ),
            Err(error) => println!("objectives: did not open - {error}"),
        }
    }

    pub(crate) fn toggle_objectives(&mut self) {
        if !self.campaign {
            println!("objectives: Tab opens PlayerTribute in skirmish - no screen built for it");
        } else if self.objectives_open() {
            self.escape_objectives();
        } else {
            self.objectives_flash.new_objective = false;
            self.open_objectives();
        }
    }

    pub(crate) fn escape_objectives(&mut self) {
        if let Some(screen) = self.objectives.as_mut().filter(|o| o.is_open()) {
            screen.escape();
        }
    }

    pub(super) fn objectives_frame(&mut self, dt: Duration) {
        if self.script_ui.objectives_closes != self.objectives_closes_seen {
            self.objectives_closes_seen = self.script_ui.objectives_closes;
            self.escape_objectives();
        }
        let feed = self.objectives_feed.clone();
        let Some(screen) = self.objectives.as_mut().filter(|o| o.is_open()) else {
            return;
        };
        self.objectives_flash.new_objective = false;
        let sounds = screen.frame(dt, &feed);
        if !screen.is_open() {
            println!("objectives: closed");
        }
        self.events.extend(sounds);
    }

    pub(super) fn notification_frame(&mut self, dt: Duration) {
        let Some(host) = self.notification.as_mut() else {
            return;
        };
        let (callbacks, sounds) = host.frame(dt);
        self.notification_callbacks.extend(callbacks);
        self.events.extend(sounds);
    }

    pub(crate) fn notification_anchor(&self) -> Anchor {
        if self.objectives_open() {
            Anchor::Objectives
        } else if self.spell_session.is_some() {
            Anchor::SpellStore
        } else {
            Anchor::Main
        }
    }

    pub(crate) fn take_notification_state(&mut self) -> (f32, Vec<Callback>) {
        let width = self
            .notification
            .as_ref()
            .map_or(0.0, |host| host.message_width());
        (width, std::mem::take(&mut self.notification_callbacks))
    }

    pub(crate) fn apply_notification(&mut self, effect: Effect) {
        let Some(host) = self.notification.as_mut() else {
            return;
        };
        match effect {
            Effect::Present(entry) => host.present(&entry),
            Effect::Close => host.close(),
            Effect::Hide => host.hide(),
        }
    }

    pub(super) fn objectives_flash_frame(&mut self) {
        let flash = self.objectives_flash;
        if flash.new_objective != flash.sent_new {
            self.palantir.expose.call(
                "objectives_flash",
                "FlashObjectivesButton",
                vec![Value::str(if flash.new_objective {
                    FLASH_ON
                } else {
                    FLASH_OFF
                })],
            );
            self.objectives_flash.sent_new = flash.new_objective;
        }
        if flash.timed != flash.sent_timed {
            self.palantir.expose.call(
                "objectives_flash_effect",
                "SetObjectivesButtonFlashEffectState",
                vec![Value::str(if flash.timed {
                    EFFECT_SHOW
                } else {
                    EFFECT_HIDE
                })],
            );
            self.objectives_flash.sent_timed = flash.timed;
        }
    }

    pub(super) fn top_pointer_move(&mut self, position: [f32; 2]) -> Option<bool> {
        if let Some(host) = self.notification.as_mut().filter(|n| n.active()) {
            let view = &mut host.hosted_mut().view;
            let stage = view.resolve_stage_point(position);
            let _ = view.player.pointer_move(stage[0], stage[1]);
            if view.player.hovered().is_some() {
                self.help_hover = None;
                self.radial_hover = None;
                return Some(true);
            }
        }
        let screen = self.objectives.as_mut().filter(|o| o.is_open())?;
        let view = &mut screen.hosted_mut().view;
        let stage = view.resolve_stage_point(position);
        let _ = view.player.pointer_move(stage[0], stage[1]);
        self.help_hover = None;
        self.radial_hover = None;
        Some(true)
    }

    pub(super) fn top_pointer_button(&mut self, pressed: bool) -> bool {
        let target = if pressed {
            let box_hovered = self
                .notification
                .as_ref()
                .is_some_and(|n| n.active() && n.hosted().view.player.hovered().is_some());
            let target = if box_hovered {
                Some(TopTarget::Notification)
            } else if self.objectives_open() {
                Some(TopTarget::Objectives)
            } else {
                None
            };
            self.top_held = target;
            target
        } else {
            self.top_held.take()
        };
        let hosted = match target {
            Some(TopTarget::Objectives) => self.objectives.as_mut().map(|o| o.hosted_mut()),
            Some(TopTarget::Notification) => self.notification.as_mut().map(|n| n.hosted_mut()),
            None => None,
        };
        let Some(hosted) = hosted else {
            return false;
        };
        let _ = hosted.view.player.pointer_button(pressed);
        true
    }

    pub(super) fn top_hosted_mut(&mut self) -> impl Iterator<Item = &mut Hosted> {
        self.objectives
            .as_mut()
            .map(|o| o.hosted_mut())
            .into_iter()
            .chain(self.notification.as_mut().map(|n| n.hosted_mut()))
    }

    pub(super) fn apply_top_images(&mut self, renderer: &mut Renderer, hud_hidden: bool) {
        if let Some(screen) = self.objectives.as_mut().filter(|o| o.is_open()) {
            screen.hosted_mut().apply_images(&mut self.images, renderer);
        }
        if let (Some(host), false) = (
            self.notification.as_mut().filter(|n| n.active()),
            hud_hidden,
        ) {
            host.hosted_mut().apply_images(&mut self.images, renderer);
        }
    }

    pub(super) fn build_top(&mut self, renderer: &mut Renderer, hud_hidden: bool) {
        if let Some(screen) = self.objectives.as_mut().filter(|o| o.is_open()) {
            self.view3d.draw(renderer, &mut screen.hosted_mut().view);
            screen.hosted_mut().view.build(renderer);
        }
        if let (Some(host), false) = (
            self.notification.as_mut().filter(|n| n.active()),
            hud_hidden,
        ) {
            host.hosted_mut().view.build(renderer);
        }
    }

    pub(super) fn top_draws(&self, hud_hidden: bool) -> Vec<ScreenDraw<'_>> {
        let mut all = Vec::new();
        if let Some(screen) = self.objectives.as_ref().filter(|o| o.is_open()) {
            all.extend(screen.hosted().view.draws());
        }
        if let (Some(host), false) = (
            self.notification.as_ref().filter(|n| n.active()),
            hud_hidden,
        ) {
            all.extend(host.hosted().view.draws());
        }
        all
    }
}
