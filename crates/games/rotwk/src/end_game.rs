use std::time::Duration;

pub(crate) const HIDE_AFTER: Duration = Duration::from_millis(7000);

pub(crate) const CAPTION_VARIABLE: &str = ":VictoryDefeat";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndScreen {
    Victory,
    Defeat,
}

impl EndScreen {
    pub(crate) const fn caption_label(self) -> &'static str {
        match self {
            Self::Victory => "APT:EndVictorious",
            Self::Defeat => "APT:EndDefeat",
        }
    }

    pub(crate) const fn sounds(self, evil: bool) -> (&'static str, &'static str) {
        match (self, evil) {
            (Self::Victory, false) => ("Gui_VictoryScreen", "Gui_VictoryCheerGood"),
            (Self::Victory, true) => ("Gui_VictoryScreen", "Gui_VictoryCheerEvil"),
            (Self::Defeat, _) => ("Gui_DefeatScreen", ""),
        }
    }

    pub(crate) fn show_arguments(self, evil: bool) -> [String; 3] {
        let (first, second) = self.sounds(evil);
        [
            String::from(if evil { "0" } else { "1" }),
            first.to_owned(),
            second.to_owned(),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndStep {
    Show(EndScreen),
    Hide,
}

#[derive(Debug, Default)]
pub(crate) struct EndGame {
    victory_fired: bool,
    defeat_fired: bool,
    shown: Option<(EndScreen, Duration)>,
    hidden: bool,
}

impl EndGame {
    pub(crate) fn step(
        &mut self,
        allied_victory: Option<bool>,
        local_defeated: bool,
        dt: Duration,
    ) -> Option<EndStep> {
        if let Some((screen, since)) = self.shown {
            let since = since.saturating_add(dt);
            if since > HIDE_AFTER {
                self.shown = None;
                self.hidden = true;
                return Some(EndStep::Hide);
            }
            self.shown = Some((screen, since));
        }
        let wanted = if allied_victory == Some(true) && !self.victory_fired {
            self.victory_fired = true;
            Some(EndScreen::Victory)
        } else if (allied_victory == Some(false) || local_defeated) && !self.defeat_fired {
            self.defeat_fired = true;
            Some(EndScreen::Defeat)
        } else {
            None
        };
        match wanted {
            Some(screen) if self.shown.is_none() => {
                self.shown = Some((screen, Duration::ZERO));
                self.hidden = false;
                Some(EndStep::Show(screen))
            }
            _ => None,
        }
    }

    pub(crate) fn show_victory_screen(&mut self) -> Option<EndStep> {
        if self.shown.is_some() {
            return None;
        }
        self.shown = Some((EndScreen::Victory, Duration::ZERO));
        self.hidden = false;
        Some(EndStep::Show(EndScreen::Victory))
    }

    #[cfg(test)]
    pub(crate) fn showing(&self) -> Option<EndScreen> {
        self.shown.map(|(screen, _)| screen)
    }

    pub(crate) const fn started(&self) -> bool {
        self.victory_fired || self.defeat_fired
    }

    pub(crate) const fn hidden(&self) -> bool {
        self.hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(33);

    #[test]
    fn a_win_shows_victory_once_and_hides_after_seven_seconds() {
        let mut end = EndGame::default();
        assert_eq!(end.step(None, false, FRAME), None);
        assert_eq!(
            end.step(Some(true), false, FRAME),
            Some(EndStep::Show(EndScreen::Victory))
        );
        let mut waited = Duration::ZERO;
        let mut hid_at = None;
        while waited < Duration::from_secs(8) {
            waited += FRAME;
            if end.step(Some(true), false, FRAME) == Some(EndStep::Hide) {
                hid_at = Some(waited);
                break;
            }
        }
        let hid_at = hid_at.expect("the screen hides");
        assert!(hid_at > HIDE_AFTER && hid_at <= HIDE_AFTER + FRAME * 2);
        assert!(end.hidden());
        assert_eq!(end.step(Some(true), false, FRAME), None);
    }

    #[test]
    fn a_victory_screen_request_while_one_is_up_does_nothing() {
        let mut end = EndGame::default();
        assert_eq!(
            end.show_victory_screen(),
            Some(EndStep::Show(EndScreen::Victory))
        );
        assert_eq!(end.show_victory_screen(), None);
    }

    #[test]
    fn local_defeat_before_the_match_is_decided_shows_defeat() {
        let mut end = EndGame::default();
        assert_eq!(
            end.step(None, true, FRAME),
            Some(EndStep::Show(EndScreen::Defeat))
        );
        assert_eq!(end.step(Some(false), true, FRAME), None);
    }

    #[test]
    fn an_allied_win_while_the_defeat_screen_is_up_is_dropped() {
        let mut end = EndGame::default();
        end.step(None, true, FRAME);
        assert_eq!(end.step(Some(true), true, FRAME), None);
        assert_eq!(end.showing(), Some(EndScreen::Defeat));
    }
}
