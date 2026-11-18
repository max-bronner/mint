use crate::campaign_flow::FADE_IN_GAME_MOVIE;
use crate::transition::TransitionHandler;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MovieFade {
    latched: bool,
    started: bool,
}

impl MovieFade {
    pub(crate) fn latch(&mut self) {
        self.latched = true;
    }

    pub(crate) fn started(self) -> bool {
        self.started
    }

    pub(crate) fn step(
        &mut self,
        handler: &mut TransitionHandler,
        enabled: bool,
        at_mark: bool,
        rendered: bool,
    ) -> bool {
        if !enabled {
            return false;
        }
        if !self.started && (self.latched || at_mark) {
            self.started = true;
            handler.set_group(FADE_IN_GAME_MOVIE);
            handler.set_hold(true);
            handler.update();
        } else if rendered {
            handler.update();
        }
        self.latched && self.started && handler.is_finished()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esc_runs_the_whole_group_one_step_per_drawn_frame_before_stopping() {
        let mut handler = TransitionHandler::with_test_group(FADE_IN_GAME_MOVIE, 5);
        let mut fade = MovieFade::default();
        assert!(!fade.step(&mut handler, true, false, true));
        fade.latch();
        let mut stopped_after = 0;
        for drawn in 1..=10 {
            let frames_before = drawn;
            let stop = fade.step(&mut handler, true, false, drawn > 1);
            if stop {
                stopped_after = frames_before;
                break;
            }
        }
        assert_eq!(stopped_after, 5);
    }

    #[test]
    fn the_fade_in_clears_one_step_per_drawn_frame_before_any_end_fade() {
        let mut handler = TransitionHandler::with_test_group(FADE_IN_GAME_MOVIE, 5);
        handler.reverse(FADE_IN_GAME_MOVIE);
        assert!(handler.screen_alpha() > 0.99);
        let mut fade = MovieFade::default();
        for _ in 0..10 {
            assert!(!fade.step(&mut handler, true, false, true));
        }
        assert!(!fade.started());
        assert!(handler.screen_alpha() < 0.01, "{}", handler.screen_alpha());
    }

    #[test]
    fn an_undrawn_frame_does_not_advance_the_fade() {
        let mut handler = TransitionHandler::with_test_group(FADE_IN_GAME_MOVIE, 5);
        let mut fade = MovieFade::default();
        fade.latch();
        fade.step(&mut handler, true, false, false);
        for _ in 0..20 {
            assert!(!fade.step(&mut handler, true, false, false));
        }
    }
}
