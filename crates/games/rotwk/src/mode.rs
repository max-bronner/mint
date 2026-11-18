#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Boot,
    Shell,
    Loading,
    Match,
    Results,
}

#[must_use]
pub(crate) const fn steps_simulation(mode: Mode) -> bool {
    matches!(mode, Mode::Match | Mode::Results)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    Booted,
    MapChosen,
    SceneReady,
    MatchEnded,
    Acknowledged,
}

#[must_use]
pub(crate) fn transition(mode: Mode, event: Event) -> Option<Mode> {
    match (mode, event) {
        (Mode::Boot, Event::Booted) => Some(Mode::Shell),
        (Mode::Shell, Event::MapChosen) => Some(Mode::Loading),
        (Mode::Loading, Event::SceneReady) => Some(Mode::Match),
        (Mode::Match, Event::MatchEnded) => Some(Mode::Results),
        (Mode::Results, Event::Acknowledged) => Some(Mode::Shell),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_cycle_reaches_every_mode_in_the_documented_order() {
        let mut mode = Mode::Boot;
        for (event, expected) in [
            (Event::Booted, Mode::Shell),
            (Event::MapChosen, Mode::Loading),
            (Event::SceneReady, Mode::Match),
            (Event::MatchEnded, Mode::Results),
            (Event::Acknowledged, Mode::Shell),
        ] {
            mode = transition(mode, event)
                .unwrap_or_else(|| panic!("{mode:?} + {event:?} should transition, did not"));
            assert_eq!(mode, expected);
        }
    }

    #[test]
    fn shell_to_match_to_results_to_shell_repeats_cleanly() {
        let mut mode = Mode::Shell;
        for _ in 0..3 {
            mode = transition(mode, Event::MapChosen).unwrap();
            assert_eq!(mode, Mode::Loading);
            mode = transition(mode, Event::SceneReady).unwrap();
            assert_eq!(mode, Mode::Match);
            mode = transition(mode, Event::MatchEnded).unwrap();
            assert_eq!(mode, Mode::Results);
            mode = transition(mode, Event::Acknowledged).unwrap();
            assert_eq!(mode, Mode::Shell);
        }
    }

    #[test]
    fn an_event_that_does_not_apply_to_the_current_mode_is_ignored() {
        let every_mode = [
            Mode::Boot,
            Mode::Shell,
            Mode::Loading,
            Mode::Match,
            Mode::Results,
        ];
        let every_event = [
            Event::Booted,
            Event::MapChosen,
            Event::SceneReady,
            Event::MatchEnded,
            Event::Acknowledged,
        ];
        let mut checked_at_least_one_none = false;
        for mode in every_mode {
            for event in every_event {
                match transition(mode, event) {
                    Some(next) => {
                        assert_ne!(next, mode, "{mode:?} + {event:?} should not self-loop");
                    }
                    None => checked_at_least_one_none = true,
                }
            }
        }
        assert!(
            checked_at_least_one_none,
            "the 5x5 table should have rejections in it, or every event applies everywhere"
        );
    }

    #[test]
    fn nothing_transitions_back_into_boot() {
        for mode in [Mode::Shell, Mode::Loading, Mode::Match, Mode::Results] {
            for event in [
                Event::Booted,
                Event::MapChosen,
                Event::SceneReady,
                Event::MatchEnded,
                Event::Acknowledged,
            ] {
                assert_ne!(transition(mode, event), Some(Mode::Boot));
            }
        }
    }
}
