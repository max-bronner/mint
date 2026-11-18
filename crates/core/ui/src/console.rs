use std::collections::VecDeque;

use crate::input::WidgetId;
use crate::list::List;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    #[must_use]
    pub fn color(self) -> [f32; 4] {
        match self {
            Severity::Info => [0.82, 0.83, 0.85, 1.0],
            Severity::Warning => [0.95, 0.76, 0.24, 1.0],
            Severity::Error => [0.92, 0.32, 0.32, 1.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConsoleLine {
    pub text: String,
    pub severity: Severity,
}

pub const MAX_SCROLLBACK_LINES: usize = 1000;

#[derive(Debug, Clone)]
pub struct Console {
    open: bool,
    input: String,
    lines: VecDeque<ConsoleLine>,
    capacity: usize,
}

impl Console {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(MAX_SCROLLBACK_LINES)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            open: false,
            input: String::new(),
            lines: VecDeque::new(),
            capacity,
        }
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn toggle_open(&mut self) {
        self.open = !self.open;
    }

    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn input_mut(&mut self) -> &mut String {
        &mut self.input
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub fn line(&self, index: usize) -> Option<&ConsoleLine> {
        self.lines.get(index)
    }

    pub fn lines(&self) -> impl Iterator<Item = &ConsoleLine> {
        self.lines.iter()
    }

    pub fn push_line(&mut self, severity: Severity, text: impl Into<String>) {
        self.lines.push_back(ConsoleLine {
            text: text.into(),
            severity,
        });
        while self.lines.len() > self.capacity {
            self.lines.pop_front();
        }
    }

    pub fn clear_lines(&mut self) {
        self.lines.clear();
    }

    #[must_use]
    pub fn scrollback_list(
        &self,
        id: WidgetId,
        viewport: crate::input::Rect,
        item_height: f32,
    ) -> List {
        List::new(id, viewport, item_height, self.line_count())
    }
}

impl Default for Console {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Rect;
    use crate::state::WidgetState;
    use crate::text_input::TextInput;

    fn viewport() -> Rect {
        Rect {
            min: [0.0, 0.0],
            max: [200.0, 100.0],
        }
    }

    #[test]
    fn a_new_console_is_closed_empty_and_has_no_lines() {
        let console = Console::new();
        assert!(!console.is_open());
        assert_eq!(console.input(), "");
        assert_eq!(console.line_count(), 0);
    }

    #[test]
    fn toggle_open_flips_and_open_close_set_it_directly() {
        let mut console = Console::new();
        console.toggle_open();
        assert!(console.is_open());
        console.toggle_open();
        assert!(!console.is_open());

        console.open();
        assert!(console.is_open());
        console.close();
        assert!(!console.is_open());
    }

    #[test]
    fn closing_does_not_clear_the_input_line_or_the_scrollback() {
        let mut console = Console::new();
        console.input_mut().push_str("still typing");
        console.push_line(Severity::Info, "earlier output");
        console.open();
        console.close();
        assert_eq!(console.input(), "still typing");
        assert_eq!(console.line_count(), 1);
    }

    #[test]
    fn a_mixed_case_punctuated_string_round_trips_through_the_console() {
        let typed = "Hello, World! 123 <mixedCASE-\"quoted\">";
        let field = TextInput::new(WidgetId(1), viewport());
        let mut console = Console::new();
        let mut state = WidgetState::new();

        for c in typed.chars() {
            field.insert_char(&mut state, console.input_mut(), c);
        }
        assert_eq!(console.input(), typed);

        let submitted = field.submit(&mut state, console.input_mut());
        assert_eq!(submitted, typed);
        assert_eq!(
            console.input(),
            "",
            "submit must drain the console's own buffer"
        );
    }

    #[test]
    fn pushed_lines_are_read_back_oldest_first_with_their_severity() {
        let mut console = Console::new();
        console.push_line(Severity::Info, "started");
        console.push_line(Severity::Error, "failed to load map \"foo\"");

        assert_eq!(console.line_count(), 2);
        assert_eq!(console.line(0).unwrap().text, "started");
        assert_eq!(console.line(0).unwrap().severity, Severity::Info);
        assert_eq!(console.line(1).unwrap().severity, Severity::Error);
        assert!(console.line(2).is_none());
    }

    #[test]
    fn the_scrollback_evicts_the_oldest_line_once_the_cap_is_reached() {
        let mut console = Console::with_capacity(3);
        for i in 0..3 {
            console.push_line(Severity::Info, format!("line {i}"));
        }
        assert_eq!(console.line_count(), 3);

        console.push_line(Severity::Info, "line 3");
        assert_eq!(console.line_count(), 3, "must not exceed the cap");
        assert_eq!(
            console.line(0).unwrap().text,
            "line 1",
            "line 0 was evicted - oldest first"
        );
        assert_eq!(
            console.line(2).unwrap().text,
            "line 3",
            "the newest line survives"
        );
    }

    #[test]
    fn clear_lines_empties_the_scrollback_without_touching_the_input() {
        let mut console = Console::new();
        console.push_line(Severity::Info, "a");
        console.input_mut().push_str("typing");
        console.clear_lines();
        assert_eq!(console.line_count(), 0);
        assert_eq!(console.input(), "typing");
    }

    #[test]
    fn scrollback_list_reports_only_the_lines_that_currently_exist() {
        let mut console = Console::with_capacity(50);
        for i in 0..5 {
            console.push_line(Severity::Info, format!("line {i}"));
        }
        let state = WidgetState::new();
        let list = console.scrollback_list(WidgetId(9), viewport(), 10.0);
        assert_eq!(list.visible_rows(&state), 0..5);
    }

    #[test]
    fn pushing_many_lines_never_grows_widgetstate_past_the_two_ids_in_use() {
        let mut console = Console::with_capacity(50);
        let mut state = WidgetState::new();
        let field = TextInput::new(WidgetId(1), viewport());
        let list_id = WidgetId(2);

        for i in 0..5_000 {
            console.push_line(Severity::Info, format!("line {i}"));
            field.insert_char(&mut state, console.input_mut(), 'x');
            let list = console.scrollback_list(list_id, viewport(), 10.0);
            list.scroll_by(&mut state, 1.0);
        }

        assert_eq!(console.line_count(), 50, "the scrollback cap still holds");
        assert_eq!(
            state.tracked_ids(),
            2,
            "exactly the field's id and the list's id - never one per line, \
             however many lines were pushed or evicted"
        );
    }
}
