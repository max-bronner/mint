use std::collections::BTreeMap;

use crate::input::WidgetId;

#[derive(Debug, Clone, Default)]
pub struct WidgetState {
    scroll: BTreeMap<WidgetId, f32>,
    caret: BTreeMap<WidgetId, usize>,
    selected: BTreeMap<WidgetId, WidgetId>,
}

impl WidgetState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn scroll_offset(&self, id: WidgetId) -> f32 {
        self.scroll.get(&id).copied().unwrap_or(0.0)
    }

    pub fn scroll_by(&mut self, id: WidgetId, delta: f32, max: f32) {
        let max = max.max(0.0);
        let next = (self.scroll_offset(id) + delta).clamp(0.0, max);
        self.scroll.insert(id, next);
    }

    pub fn set_scroll(&mut self, id: WidgetId, offset: f32, max: f32) {
        let max = max.max(0.0);
        self.scroll.insert(id, offset.clamp(0.0, max));
    }

    #[must_use]
    pub fn caret(&self, id: WidgetId) -> usize {
        self.caret.get(&id).copied().unwrap_or(0)
    }

    pub fn set_caret(&mut self, id: WidgetId, position: usize) {
        self.caret.insert(id, position);
    }

    #[must_use]
    pub fn selected(&self, group: WidgetId) -> Option<WidgetId> {
        self.selected.get(&group).copied()
    }

    pub fn select(&mut self, group: WidgetId, item: WidgetId) {
        self.selected.insert(group, item);
    }

    pub fn forget(&mut self, id: WidgetId) {
        self.scroll.remove(&id);
        self.caret.remove(&id);
        self.selected.remove(&id);
    }

    #[must_use]
    pub fn tracked_ids(&self) -> usize {
        let mut ids: std::collections::BTreeSet<WidgetId> = self.scroll.keys().copied().collect();
        ids.extend(self.caret.keys().copied());
        ids.extend(self.selected.keys().copied());
        ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_widget_has_zero_offset() {
        let state = WidgetState::new();
        assert_eq!(state.scroll_offset(WidgetId(1)), 0.0);
    }

    #[test]
    fn scrolling_clamps_to_the_given_range() {
        let mut state = WidgetState::new();
        let id = WidgetId(1);

        state.scroll_by(id, 500.0, 100.0);
        assert_eq!(state.scroll_offset(id), 100.0, "must clamp to max");

        state.scroll_by(id, -1000.0, 100.0);
        assert_eq!(state.scroll_offset(id), 0.0, "must clamp to zero");
    }

    #[test]
    fn set_scroll_jumps_directly_and_still_clamps() {
        let mut state = WidgetState::new();
        let id = WidgetId(1);
        state.set_scroll(id, 40.0, 100.0);
        assert_eq!(state.scroll_offset(id), 40.0);
        state.set_scroll(id, 999.0, 100.0);
        assert_eq!(state.scroll_offset(id), 100.0);
    }

    #[test]
    fn distinct_widgets_scroll_independently() {
        let mut state = WidgetState::new();
        state.scroll_by(WidgetId(1), 30.0, 100.0);
        state.scroll_by(WidgetId(2), 10.0, 100.0);
        assert_eq!(state.scroll_offset(WidgetId(1)), 30.0);
        assert_eq!(state.scroll_offset(WidgetId(2)), 10.0);
    }

    #[test]
    fn forget_resets_a_widget_to_its_untouched_default() {
        let mut state = WidgetState::new();
        let id = WidgetId(1);
        state.scroll_by(id, 50.0, 100.0);
        assert_eq!(state.scroll_offset(id), 50.0);
        state.forget(id);
        assert_eq!(state.scroll_offset(id), 0.0);
    }

    #[test]
    fn an_untouched_widget_has_a_caret_at_the_start() {
        let state = WidgetState::new();
        assert_eq!(state.caret(WidgetId(1)), 0);
    }

    #[test]
    fn set_caret_is_remembered_and_forget_clears_it_too() {
        let mut state = WidgetState::new();
        let id = WidgetId(1);
        state.set_caret(id, 4);
        assert_eq!(state.caret(id), 4);
        state.forget(id);
        assert_eq!(
            state.caret(id),
            0,
            "forget must clear caret, not only scroll"
        );
    }

    #[test]
    fn an_untouched_group_has_no_selection() {
        let state = WidgetState::new();
        assert_eq!(state.selected(WidgetId(1)), None);
    }

    #[test]
    fn selecting_replaces_the_previous_selection_in_the_same_group() {
        let mut state = WidgetState::new();
        let group = WidgetId(1);
        state.select(group, WidgetId(10));
        assert_eq!(state.selected(group), Some(WidgetId(10)));
        state.select(group, WidgetId(11));
        assert_eq!(
            state.selected(group),
            Some(WidgetId(11)),
            "a group is single-select"
        );
    }

    #[test]
    fn distinct_groups_select_independently_and_forget_clears_selection_too() {
        let mut state = WidgetState::new();
        state.select(WidgetId(1), WidgetId(10));
        state.select(WidgetId(2), WidgetId(20));
        assert_eq!(state.selected(WidgetId(1)), Some(WidgetId(10)));
        assert_eq!(state.selected(WidgetId(2)), Some(WidgetId(20)));

        state.forget(WidgetId(1));
        assert_eq!(
            state.selected(WidgetId(1)),
            None,
            "forget must clear selection, not only scroll and caret"
        );
        assert_eq!(
            state.selected(WidgetId(2)),
            Some(WidgetId(20)),
            "forgetting one group must not disturb another"
        );
    }

    #[test]
    fn tracked_ids_counts_each_widget_once_across_both_maps() {
        let mut state = WidgetState::new();
        assert_eq!(state.tracked_ids(), 0);

        state.scroll_by(WidgetId(1), 10.0, 100.0);
        assert_eq!(state.tracked_ids(), 1);

        state.set_caret(WidgetId(1), 3);
        assert_eq!(state.tracked_ids(), 1);

        state.set_caret(WidgetId(2), 1);
        assert_eq!(state.tracked_ids(), 2);

        state.select(WidgetId(3), WidgetId(30));
        assert_eq!(state.tracked_ids(), 3);

        state.forget(WidgetId(1));
        assert_eq!(state.tracked_ids(), 2);
    }
}
