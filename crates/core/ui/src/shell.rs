use crate::input::{HitTarget, PointerRouting, Rect, WidgetId};
use crate::list::List;
use crate::state::WidgetState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellEntry<'a> {
    pub id: WidgetId,
    pub name: &'a str,
}

impl<'a> ShellEntry<'a> {
    #[must_use]
    pub fn new(id: WidgetId, name: &'a str) -> Self {
        Self { id, name }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellAction {
    None,
    Selected(usize),
    Start,
}

#[derive(Debug, Clone, Copy)]
pub struct Shell<'a> {
    pub id: WidgetId,
    pub viewport: Rect,
    pub entries: &'a [ShellEntry<'a>],
    pub item_height: f32,
    pub start_id: WidgetId,
}

impl<'a> Shell<'a> {
    #[must_use]
    pub fn new(
        id: WidgetId,
        viewport: Rect,
        entries: &'a [ShellEntry<'a>],
        item_height: f32,
        start_id: WidgetId,
    ) -> Self {
        Self {
            id,
            viewport,
            entries,
            item_height,
            start_id,
        }
    }

    #[must_use]
    pub fn entry_list(&self) -> List {
        List::new(self.id, self.viewport, self.item_height, self.entries.len())
    }

    #[must_use]
    pub fn entry_hit_targets(&self, state: &WidgetState) -> Vec<HitTarget> {
        let list = self.entry_list();
        list.visible_rows(state)
            .filter_map(|index| {
                self.entries.get(index).map(|entry| HitTarget {
                    id: entry.id,
                    region: list.row_rect(index, state),
                    focusable: true,
                })
            })
            .collect()
    }

    #[must_use]
    pub fn selected_index(&self, state: &WidgetState) -> Option<usize> {
        let selected_id = state.selected(self.id)?;
        self.entries
            .iter()
            .position(|entry| entry.id == selected_id)
    }

    pub fn handle_pointer_button(
        &self,
        state: &mut WidgetState,
        routing: PointerRouting,
        pressed: bool,
    ) -> ShellAction {
        if pressed {
            return ShellAction::None;
        }
        let PointerRouting::Widget(id) = routing else {
            return ShellAction::None;
        };
        if id == self.start_id {
            return ShellAction::Start;
        }
        match self.entries.iter().position(|entry| entry.id == id) {
            Some(index) => {
                state.select(self.id, id);
                ShellAction::Selected(index)
            }
            None => ShellAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputRouter, PointerButton};

    fn viewport() -> Rect {
        Rect {
            min: [0.0, 0.0],
            max: [200.0, 100.0],
        }
    }

    fn entries() -> Vec<ShellEntry<'static>> {
        vec![
            ShellEntry::new(WidgetId(10), "Synthetic Vale"),
            ShellEntry::new(WidgetId(11), "Test Hollow"),
            ShellEntry::new(WidgetId(12), "Placeholder Reach"),
        ]
    }

    #[test]
    fn clicking_an_entry_selects_it_and_selection_is_read_back_by_index() {
        let entries = entries();
        let shell = Shell::new(WidgetId(1), viewport(), &entries, 20.0, WidgetId(99));
        let mut state = WidgetState::new();
        let mut router = InputRouter::new();

        assert_eq!(shell.selected_index(&state), None, "nothing selected yet");

        let targets = shell.entry_hit_targets(&state);
        let press = router.pointer_button(&targets, [5.0, 25.0], PointerButton::Primary, true);
        let action_on_press = shell.handle_pointer_button(&mut state, press, true);
        assert_eq!(
            action_on_press,
            ShellAction::None,
            "a press must never select - only the matching release does"
        );
        assert_eq!(shell.selected_index(&state), None, "still nothing selected");

        let release = router.pointer_button(&targets, [5.0, 25.0], PointerButton::Primary, false);
        let action = shell.handle_pointer_button(&mut state, release, false);
        assert_eq!(action, ShellAction::Selected(1));
        assert_eq!(shell.selected_index(&state), Some(1));
    }

    #[test]
    fn selecting_a_second_entry_replaces_the_first() {
        let entries = entries();
        let shell = Shell::new(WidgetId(1), viewport(), &entries, 20.0, WidgetId(99));
        let mut state = WidgetState::new();
        let mut router = InputRouter::new();
        let targets = shell.entry_hit_targets(&state);

        router.pointer_button(&targets, [5.0, 5.0], PointerButton::Primary, true);
        let r0 = router.pointer_button(&targets, [5.0, 5.0], PointerButton::Primary, false);
        shell.handle_pointer_button(&mut state, r0, false);
        assert_eq!(shell.selected_index(&state), Some(0));

        router.pointer_button(&targets, [5.0, 45.0], PointerButton::Primary, true);
        let r2 = router.pointer_button(&targets, [5.0, 45.0], PointerButton::Primary, false);
        shell.handle_pointer_button(&mut state, r2, false);
        assert_eq!(shell.selected_index(&state), Some(2));
    }

    #[test]
    fn clicking_start_fires_exactly_once_across_a_full_press_release_sequence() {
        let entries = entries();
        let start_id = WidgetId(99);
        let start_rect = Rect {
            min: [0.0, 80.0],
            max: [60.0, 100.0],
        };
        let shell = Shell::new(WidgetId(1), viewport(), &entries, 20.0, start_id);
        let mut state = WidgetState::new();
        let mut router = InputRouter::new();

        let mut targets = shell.entry_hit_targets(&state);
        targets.push(HitTarget {
            id: start_id,
            region: start_rect,
            focusable: true,
        });

        let mut callback_calls = 0usize;
        for pressed in [true, false] {
            let routing =
                router.pointer_button(&targets, [10.0, 90.0], PointerButton::Primary, pressed);
            if shell.handle_pointer_button(&mut state, routing, pressed) == ShellAction::Start {
                callback_calls += 1;
            }
        }

        assert_eq!(
            callback_calls, 1,
            "exactly one Start action across the whole press-release sequence"
        );
    }

    #[test]
    fn a_press_never_produces_start_only_a_release_does() {
        let entries = entries();
        let start_id = WidgetId(99);
        let shell = Shell::new(WidgetId(1), viewport(), &entries, 20.0, start_id);
        let mut state = WidgetState::new();
        let routing = PointerRouting::Widget(start_id);

        assert_eq!(
            shell.handle_pointer_button(&mut state, routing, true),
            ShellAction::None
        );
        assert_eq!(
            shell.handle_pointer_button(&mut state, routing, false),
            ShellAction::Start
        );
    }

    #[test]
    fn a_world_routing_never_selects_or_starts() {
        let entries = entries();
        let shell = Shell::new(WidgetId(1), viewport(), &entries, 20.0, WidgetId(99));
        let mut state = WidgetState::new();
        assert_eq!(
            shell.handle_pointer_button(&mut state, PointerRouting::World, false),
            ShellAction::None
        );
    }

    #[test]
    fn entry_hit_targets_are_culled_to_the_viewport() {
        let many: Vec<ShellEntry<'static>> = (0..1_000)
            .map(|i| ShellEntry::new(WidgetId(1000 + i), "Synthetic Map"))
            .collect();
        let shell = Shell::new(WidgetId(1), viewport(), &many, 10.0, WidgetId(99));
        let state = WidgetState::new();
        let targets = shell.entry_hit_targets(&state);
        assert!(
            targets.len() <= 10,
            "a 100px viewport with 10px rows must not hit-test all 1000 entries"
        );
    }
}
