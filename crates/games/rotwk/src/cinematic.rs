use mint_app::RtsCamera;

use crate::letterbox::Letterbox;
use crate::military_caption::{Caption, CaptionStyle};
use crate::rules::map_scripts::CineState;

#[derive(Debug, Default)]
pub(crate) struct Cinematic {
    pub(crate) letterbox: Letterbox,
    pub(crate) caption: Option<Caption>,
    caption_seen: u32,
    pub(crate) radar_seen: u32,
    pub(crate) dev: Option<CineState>,
    pub(crate) objective_sound_seen: u32,
    pub(crate) ui_flash_seen: u32,
    pub(crate) object_flash_seen: u32,
    pub(crate) movie_seen: u32,
    pub(crate) notification: crate::notification_box::Manager,
    pub(crate) notification_seen: u32,
    pub(crate) objectives_shown: Vec<bool>,
    pub(crate) button_flashes: crate::button_flash::ButtonFlashes,
    frozen_at: Option<f32>,
    frozen_total: f32,
    pub(crate) style: CaptionStyle,
}

impl Cinematic {
    pub(crate) fn client_clock(&mut self, frozen: bool, clock: f32) -> f32 {
        if frozen {
            let at = *self.frozen_at.get_or_insert(clock);
            return at - self.frozen_total;
        }
        if let Some(at) = self.frozen_at.take() {
            self.frozen_total += clock - at;
        }
        clock - self.frozen_total
    }

    pub(crate) fn drive(
        &mut self,
        cine: &CineState,
        style: &CaptionStyle,
        text_of: impl Fn(&str) -> String,
        camera: &mut RtsCamera,
        dt: f32,
        menu_up: bool,
    ) -> u32 {
        self.style = style.clone();
        self.letterbox.update(cine.letterbox, dt, camera);
        if let Some(command) = cine.caption.as_ref().filter(|c| c.seq != self.caption_seen) {
            self.caption_seen = command.seq;
            self.caption = Some(Caption::new(
                &text_of(&command.label),
                u32::try_from(command.ms).unwrap_or(0),
                style.delay_ms,
            ));
        }
        let Some(caption) = self.caption.as_mut() else {
            return 0;
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let since_last_ms = (dt.max(0.0) * 1000.0) as u32;
        let step = caption.update(since_last_ms, menu_up);
        if step.removed {
            self.caption = None;
        }
        step.clicks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_clock_stands_still_while_frozen_and_resumes_where_it_stopped() {
        let mut cinematic = Cinematic::default();
        assert_eq!(cinematic.client_clock(false, 10.0), 10.0);
        assert_eq!(cinematic.client_clock(true, 11.0), 11.0);
        assert_eq!(cinematic.client_clock(true, 14.0), 11.0);
        assert_eq!(cinematic.client_clock(false, 15.0), 11.0);
        assert_eq!(cinematic.client_clock(false, 16.0), 12.0);
    }
}
