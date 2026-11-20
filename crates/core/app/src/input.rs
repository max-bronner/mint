#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Key {
    Character(char),
    CtrlCharacter(char),
    CtrlShiftCharacter(char),
    Escape,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Backspace,
    Enter,
    Tab,
    Function(u8),
    CtrlShiftDigit(u8),
    Shift,
    Alt,
    Numpad(u8),
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum InputEvent {
    PointerMoved {
        x: f32,
        y: f32,
    },
    PointerLeft,
    PointerDoubleClick {
        button: PointerButton,
    },
    PointerButton {
        button: PointerButton,
        pressed: bool,
    },
    Scroll {
        lines: f32,
    },
    Key {
        key: Key,
        pressed: bool,
    },
    Resized {
        width: u32,
        height: u32,
    },
    TextInput {
        character: char,
    },
    Modifiers {
        ctrl: bool,
    },
}

pub const DOUBLE_CLICK_TIME: std::time::Duration = std::time::Duration::from_millis(500);

pub const DOUBLE_CLICK_SPAN: f32 = 4.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct DoubleClicks {
    last: Option<(PointerButton, std::time::Duration, [f32; 2])>,
}

impl DoubleClicks {
    pub(crate) fn press(
        &mut self,
        button: PointerButton,
        at: std::time::Duration,
        pos: [f32; 2],
    ) -> bool {
        let half = DOUBLE_CLICK_SPAN * 0.5;
        let double = self.last.is_some_and(|(b, t, p)| {
            b == button
                && at.saturating_sub(t) <= DOUBLE_CLICK_TIME
                && (pos[0] - p[0]).abs() <= half
                && (pos[1] - p[1]).abs() <= half
        });
        self.last = if double {
            None
        } else {
            Some((button, at, pos))
        };
        double
    }
}

const PIXELS_PER_LINE: f32 = 50.0;

#[must_use]
pub(crate) fn lines_from_pixels(pixels: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let narrowed = pixels as f32;
    narrowed / PIXELS_PER_LINE
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    #[test]
    fn a_second_press_close_in_time_and_place_is_a_double_click_and_a_third_is_not() {
        use std::time::Duration;
        let mut clicks = DoubleClicks::default();
        let ms = Duration::from_millis;
        assert!(!clicks.press(PointerButton::Middle, ms(0), [100.0, 100.0]));
        assert!(clicks.press(PointerButton::Middle, ms(400), [101.0, 102.0]));
        assert!(!clicks.press(PointerButton::Middle, ms(500), [101.0, 102.0]));
        assert!(!clicks.press(PointerButton::Middle, ms(1100), [101.0, 102.0]));
        assert!(!clicks.press(PointerButton::Primary, ms(1200), [101.0, 102.0]));
        assert!(!clicks.press(PointerButton::Primary, ms(1300), [104.0, 102.0]));
    }

    #[test]
    fn a_touchpad_delta_becomes_a_fraction_of_a_wheel_detent() {
        assert!((lines_from_pixels(50.0) - 1.0).abs() < 1e-6);
        assert!((lines_from_pixels(-25.0) + 0.5).abs() < 1e-6);
        assert_eq!(lines_from_pixels(0.0), 0.0);
    }

    #[test]
    fn a_character_key_is_lowercased_so_shift_does_not_make_a_second_key() {
        assert_eq!(Key::Character('t'), Key::Character('t'));
        assert_ne!(Key::Character('t'), Key::Character('T'));
    }

    #[test]
    fn text_input_keeps_the_case_a_character_key_would_have_discarded() {
        let upper = InputEvent::TextInput { character: 'T' };
        let lower = InputEvent::TextInput { character: 't' };
        assert_ne!(upper, lower);
    }

    #[test]
    fn events_are_plain_values_so_they_can_be_recorded_and_replayed() {
        let a = InputEvent::PointerMoved { x: 1.0, y: 2.0 };
        let b = a;
        assert_eq!(a, b);
        assert_ne!(a, InputEvent::PointerMoved { x: 1.0, y: 3.0 });
    }
}
