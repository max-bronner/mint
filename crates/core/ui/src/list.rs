use std::ops::Range;

use crate::input::{Rect, WidgetId};
use crate::state::WidgetState;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn floor_to_row_index(value: f32) -> usize {
    value.max(0.0).floor() as usize
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn ceil_to_row_index(value: f32) -> usize {
    value.max(0.0).ceil() as usize
}

#[allow(clippy::cast_precision_loss)]
fn row_index_as_f32(index: usize) -> f32 {
    index as f32
}

#[derive(Debug, Clone, Copy)]
pub struct List {
    pub id: WidgetId,
    pub viewport: Rect,
    pub item_height: f32,
    pub item_count: usize,
}

impl List {
    #[must_use]
    pub fn new(id: WidgetId, viewport: Rect, item_height: f32, item_count: usize) -> Self {
        Self {
            id,
            viewport,
            item_height,
            item_count,
        }
    }

    #[must_use]
    pub fn max_scroll(&self) -> f32 {
        if self.item_height <= 0.0 {
            return 0.0;
        }
        let viewport_height = (self.viewport.max[1] - self.viewport.min[1]).max(0.0);
        let content_height = self.item_height * row_index_as_f32(self.item_count);
        (content_height - viewport_height).max(0.0)
    }

    #[must_use]
    pub fn visible_rows(&self, state: &WidgetState) -> Range<usize> {
        if self.item_count == 0 || self.item_height <= 0.0 {
            return 0..0;
        }
        let offset = state.scroll_offset(self.id).max(0.0);
        let viewport_height = (self.viewport.max[1] - self.viewport.min[1]).max(0.0);

        let first = floor_to_row_index(offset / self.item_height).min(self.item_count);
        let last = ceil_to_row_index((offset + viewport_height) / self.item_height)
            .clamp(first, self.item_count);
        first..last
    }

    #[must_use]
    pub fn row_rect(&self, index: usize, state: &WidgetState) -> Rect {
        let offset = state.scroll_offset(self.id);
        let top = self.viewport.min[1] + row_index_as_f32(index) * self.item_height - offset;
        Rect {
            min: [self.viewport.min[0], top],
            max: [self.viewport.max[0], top + self.item_height],
        }
    }

    pub fn scroll_by(&self, state: &mut WidgetState, delta: f32) {
        state.scroll_by(self.id, delta, self.max_scroll());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(item_count: usize) -> List {
        List::new(
            WidgetId(1),
            Rect {
                min: [0.0, 0.0],
                max: [200.0, 100.0],
            },
            10.0,
            item_count,
        )
    }

    #[test]
    fn visible_row_count_does_not_grow_with_the_item_count() {
        let state = WidgetState::new();
        let small = list(20).visible_rows(&state);
        let huge = list(1_000).visible_rows(&state);
        assert_eq!(small, 0..10, "100px viewport / 10px rows divides exactly");
        assert_eq!(small.len(), huge.len(), "same viewport, same visible count");
        assert!(huge.len() <= 10, "must not grow with 1,000 backing items");
    }

    #[test]
    fn a_partially_visible_last_row_is_still_counted() {
        let list = List::new(
            WidgetId(1),
            Rect {
                min: [0.0, 0.0],
                max: [200.0, 100.0],
            },
            13.0,
            1_000,
        );
        let state = WidgetState::new();
        assert_eq!(list.visible_rows(&state), 0..8);
    }

    #[test]
    fn scrolling_moves_the_visible_window_without_changing_its_size() {
        let list = list(1_000);
        let mut state = WidgetState::new();
        let before = list.visible_rows(&state);
        assert_eq!(before, 0..10);

        list.scroll_by(&mut state, 250.0);
        let after = list.visible_rows(&state);
        assert_eq!(after, 25..35);
        assert_eq!(after.len(), before.len());
    }

    #[test]
    fn scrolling_past_the_end_clamps_to_the_last_page() {
        let list = list(30);
        let mut state = WidgetState::new();
        list.scroll_by(&mut state, 1.0e6);
        let visible = list.visible_rows(&state);
        assert_eq!(visible.end, 30, "must not report rows past the last item");
        assert!(visible.start < 30);
    }

    #[test]
    fn an_empty_list_shows_nothing_and_never_divides_by_a_bad_height() {
        let state = WidgetState::new();
        assert_eq!(list(0).visible_rows(&state), 0..0);

        let zero_height = List::new(
            WidgetId(1),
            Rect {
                min: [0.0, 0.0],
                max: [200.0, 100.0],
            },
            0.0,
            1_000,
        );
        assert_eq!(zero_height.visible_rows(&state), 0..0);
        assert_eq!(zero_height.max_scroll(), 0.0);
    }

    #[test]
    fn row_rect_accounts_for_index_and_scroll_offset() {
        let list = list(1_000);
        let mut state = WidgetState::new();
        assert_eq!(
            list.row_rect(3, &state),
            Rect {
                min: [0.0, 30.0],
                max: [200.0, 40.0]
            }
        );
        list.scroll_by(&mut state, 15.0);
        assert_eq!(
            list.row_rect(3, &state),
            Rect {
                min: [0.0, 15.0],
                max: [200.0, 25.0]
            },
            "scrolling down moves every row's rect up by the same amount"
        );
    }

    #[test]
    fn max_scroll_is_zero_when_everything_already_fits() {
        let list = list(5);
        assert_eq!(list.max_scroll(), 0.0);
    }
}
