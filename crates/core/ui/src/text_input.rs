use crate::input::{HitTarget, Rect, WidgetId};
use crate::state::WidgetState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextInput {
    pub id: WidgetId,
    pub rect: Rect,
}

fn char_len(buffer: &str) -> usize {
    buffer.chars().count()
}

fn byte_offset(buffer: &str, index: usize) -> usize {
    buffer
        .char_indices()
        .nth(index)
        .map_or(buffer.len(), |(offset, _)| offset)
}

impl TextInput {
    #[must_use]
    pub fn new(id: WidgetId, rect: Rect) -> Self {
        Self { id, rect }
    }

    #[must_use]
    pub fn hit_target(&self) -> HitTarget {
        HitTarget {
            id: self.id,
            region: self.rect,
            focusable: true,
        }
    }

    #[must_use]
    pub fn caret(&self, state: &WidgetState) -> usize {
        state.caret(self.id)
    }

    pub fn insert_char(&self, state: &mut WidgetState, buffer: &mut String, c: char) {
        if c.is_control() {
            return;
        }
        let caret = state.caret(self.id).min(char_len(buffer));
        buffer.insert(byte_offset(buffer, caret), c);
        state.set_caret(self.id, caret.saturating_add(1));
    }

    pub fn backspace(&self, state: &mut WidgetState, buffer: &mut String) {
        let caret = state.caret(self.id).min(char_len(buffer));
        if caret == 0 {
            return;
        }
        let new_caret = caret.saturating_sub(1);
        let start = byte_offset(buffer, new_caret);
        let end = byte_offset(buffer, caret);
        buffer.replace_range(start..end, "");
        state.set_caret(self.id, new_caret);
    }

    pub fn delete_forward(&self, state: &mut WidgetState, buffer: &mut String) {
        let len = char_len(buffer);
        let caret = state.caret(self.id).min(len);
        if caret >= len {
            return;
        }
        let start = byte_offset(buffer, caret);
        let end = byte_offset(buffer, caret.saturating_add(1));
        buffer.replace_range(start..end, "");
    }

    pub fn move_left(&self, state: &mut WidgetState, buffer: &str) {
        let caret = state.caret(self.id).min(char_len(buffer));
        state.set_caret(self.id, caret.saturating_sub(1));
    }

    pub fn move_right(&self, state: &mut WidgetState, buffer: &str) {
        let len = char_len(buffer);
        let caret = state.caret(self.id).min(len);
        state.set_caret(self.id, caret.saturating_add(1).min(len));
    }

    pub fn move_home(&self, state: &mut WidgetState) {
        state.set_caret(self.id, 0);
    }

    pub fn move_end(&self, state: &mut WidgetState, buffer: &str) {
        state.set_caret(self.id, char_len(buffer));
    }

    pub fn submit(&self, state: &mut WidgetState, buffer: &mut String) -> String {
        state.set_caret(self.id, 0);
        std::mem::take(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field() -> TextInput {
        TextInput::new(
            WidgetId(1),
            Rect {
                min: [0.0, 0.0],
                max: [200.0, 20.0],
            },
        )
    }

    #[test]
    fn a_mixed_case_punctuated_string_round_trips_exactly() {
        let typed = "Hello, World! 123 <mixedCASE-\"quoted\">";
        let field = field();
        let mut state = WidgetState::new();
        let mut buffer = String::new();
        for c in typed.chars() {
            field.insert_char(&mut state, &mut buffer, c);
        }
        assert_eq!(buffer, typed);
        assert_eq!(field.caret(&state), typed.chars().count());
    }

    #[test]
    fn insert_char_drops_control_characters() {
        let field = field();
        let mut state = WidgetState::new();
        let mut buffer = String::new();
        field.insert_char(&mut state, &mut buffer, 'a');
        field.insert_char(&mut state, &mut buffer, '\r');
        field.insert_char(&mut state, &mut buffer, 'b');
        assert_eq!(
            buffer, "ab",
            "a control character must not become a literal byte in the buffer"
        );
        assert_eq!(field.caret(&state), 2);
    }

    #[test]
    fn backspace_removes_the_character_before_the_caret_and_moves_it_back() {
        let field = field();
        let mut state = WidgetState::new();
        let mut buffer = "abc".to_owned();
        state.set_caret(field.id, 3);

        field.backspace(&mut state, &mut buffer);
        assert_eq!(buffer, "ab");
        assert_eq!(field.caret(&state), 2);
    }

    #[test]
    fn backspace_at_the_start_does_nothing() {
        let field = field();
        let mut state = WidgetState::new();
        let mut buffer = "abc".to_owned();
        field.backspace(&mut state, &mut buffer);
        assert_eq!(buffer, "abc", "caret is already 0, nothing to remove");
    }

    #[test]
    fn backspace_and_insert_are_safe_on_multi_byte_characters() {
        let field = field();
        let mut state = WidgetState::new();
        let mut buffer = String::new();
        for c in "héllo".chars() {
            field.insert_char(&mut state, &mut buffer, c);
        }
        assert_eq!(buffer, "héllo");
        field.backspace(&mut state, &mut buffer);
        field.backspace(&mut state, &mut buffer);
        assert_eq!(
            buffer, "hél",
            "must remove whole characters, never split a UTF-8 boundary"
        );
    }

    #[test]
    fn delete_forward_removes_the_character_at_the_caret_without_moving_it() {
        let field = field();
        let mut state = WidgetState::new();
        let mut buffer = "abc".to_owned();
        state.set_caret(field.id, 1);
        field.delete_forward(&mut state, &mut buffer);
        assert_eq!(buffer, "ac");
        assert_eq!(
            field.caret(&state),
            1,
            "delete-forward does not move the caret"
        );
    }

    #[test]
    fn delete_forward_at_the_end_does_nothing() {
        let field = field();
        let mut state = WidgetState::new();
        let mut buffer = "abc".to_owned();
        state.set_caret(field.id, 3);
        field.delete_forward(&mut state, &mut buffer);
        assert_eq!(buffer, "abc");
    }

    #[test]
    fn arrow_and_home_end_movement_clamp_to_the_buffer() {
        let field = field();
        let mut state = WidgetState::new();
        let buffer = "abc".to_owned();

        field.move_left(&mut state, &buffer);
        assert_eq!(field.caret(&state), 0);

        field.move_end(&mut state, &buffer);
        assert_eq!(field.caret(&state), 3);

        field.move_right(&mut state, &buffer);
        assert_eq!(field.caret(&state), 3);

        field.move_home(&mut state);
        assert_eq!(field.caret(&state), 0);

        field.move_right(&mut state, &buffer);
        assert_eq!(field.caret(&state), 1);
    }

    #[test]
    fn submit_returns_the_buffer_and_resets_it_and_the_caret() {
        let field = field();
        let mut state = WidgetState::new();
        let mut buffer = "a command".to_owned();
        state.set_caret(field.id, 9);

        let submitted = field.submit(&mut state, &mut buffer);
        assert_eq!(submitted, "a command");
        assert_eq!(buffer, "", "the buffer is drained, not merely read");
        assert_eq!(field.caret(&state), 0);
    }

    #[test]
    fn a_stale_caret_past_the_current_buffer_length_never_panics() {
        let field = field();
        let mut state = WidgetState::new();
        state.set_caret(field.id, 500);
        let mut buffer = "ab".to_owned();

        field.insert_char(&mut state, &mut buffer, 'c');
        assert_eq!(
            buffer, "abc",
            "an out-of-range caret clamps to the end, not a panic"
        );

        state.set_caret(field.id, 500);
        field.backspace(&mut state, &mut buffer);
        assert_eq!(buffer, "ab");

        state.set_caret(field.id, 500);
        field.delete_forward(&mut state, &mut buffer);
        assert_eq!(
            buffer, "ab",
            "an out-of-range caret has nothing to delete forward"
        );
    }
}
