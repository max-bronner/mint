#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl Rect {
    #[must_use]
    pub fn contains(&self, point: [f32; 2]) -> bool {
        point[0] >= self.min[0]
            && point[0] < self.max[0]
            && point[1] >= self.min[1]
            && point[1] < self.max[1]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitTarget {
    pub id: WidgetId,
    pub region: Rect,
    pub focusable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerRouting {
    Widget(WidgetId),
    World,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputRouter {
    hovered: Option<WidgetId>,
    focused: Option<WidgetId>,
    captured: Option<(WidgetId, PointerButton)>,
}

impl InputRouter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn hovered(&self) -> Option<WidgetId> {
        self.hovered
    }

    #[must_use]
    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
    }

    #[must_use]
    pub fn captured(&self) -> Option<WidgetId> {
        self.captured.map(|(id, _button)| id)
    }

    pub fn hit_test(&mut self, targets: &[HitTarget], position: [f32; 2]) -> Option<WidgetId> {
        let hit = targets
            .iter()
            .find(|target| target.region.contains(position))
            .map(|target| target.id);
        self.hovered = hit;
        hit
    }

    pub fn pointer_moved(&mut self, targets: &[HitTarget], position: [f32; 2]) -> PointerRouting {
        let hit = self.hit_test(targets, position);
        match self.captured {
            Some((id, _button)) => PointerRouting::Widget(id),
            None => hit.map_or(PointerRouting::World, PointerRouting::Widget),
        }
    }

    pub fn pointer_button(
        &mut self,
        targets: &[HitTarget],
        position: [f32; 2],
        button: PointerButton,
        pressed: bool,
    ) -> PointerRouting {
        let hit = self.hit_test(targets, position);
        if let Some((id, captured_button)) = self.captured {
            if !pressed && button == captured_button {
                self.captured = None;
            }
            return PointerRouting::Widget(id);
        }
        match hit {
            Some(id) => {
                if pressed {
                    self.captured = Some((id, button));
                    if targets
                        .iter()
                        .any(|target| target.id == id && target.focusable)
                    {
                        self.focused = Some(id);
                    }
                }
                PointerRouting::Widget(id)
            }
            None => PointerRouting::World,
        }
    }

    pub fn release_capture(&mut self) {
        self.captured = None;
    }

    pub fn clear_focus(&mut self) {
        self.focused = None;
    }

    pub fn focus_next(&mut self, targets: &[HitTarget]) {
        self.step_focus(targets, 1);
    }

    pub fn focus_previous(&mut self, targets: &[HitTarget]) {
        self.step_focus(targets, -1);
    }

    fn step_focus(&mut self, targets: &[HitTarget], step: i64) {
        let focusable: Vec<WidgetId> = targets
            .iter()
            .filter(|target| target.focusable)
            .map(|target| target.id)
            .collect();
        if focusable.is_empty() {
            self.focused = None;
            return;
        }
        let len = i64::try_from(focusable.len()).unwrap_or(i64::MAX);
        let current = self
            .focused
            .and_then(|id| focusable.iter().position(|candidate| *candidate == id))
            .and_then(|index| i64::try_from(index).ok());
        let next = match current {
            Some(index) => index.saturating_add(step).rem_euclid(len),
            None => 0,
        };
        self.focused = usize::try_from(next)
            .ok()
            .and_then(|index| focusable.get(index))
            .copied();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: u64, min: [f32; 2], max: [f32; 2], focusable: bool) -> HitTarget {
        HitTarget {
            id: WidgetId(id),
            region: Rect { min, max },
            focusable,
        }
    }

    #[test]
    fn a_widget_on_the_left_half_claims_its_side_and_the_world_gets_the_rest() {
        let targets = [target(1, [0.0, 0.0], [32.0, 64.0], false)];
        let mut router = InputRouter::new();
        let mut world_events: Vec<[f32; 2]> = Vec::new();
        let mut widget_events: Vec<[f32; 2]> = Vec::new();

        for point in [[5.0, 5.0], [40.0, 5.0], [31.9, 63.9], [32.0, 10.0]] {
            match router.pointer_moved(&targets, point) {
                PointerRouting::Widget(WidgetId(1)) => widget_events.push(point),
                PointerRouting::World => world_events.push(point),
                other => panic!("unexpected routing {other:?} for {point:?}"),
            }
        }

        assert_eq!(widget_events, vec![[5.0, 5.0], [31.9, 63.9]]);
        assert_eq!(world_events, vec![[40.0, 5.0], [32.0, 10.0]]);
    }

    #[test]
    fn an_oversized_hit_region_claims_points_well_outside_what_it_visually_draws() {
        let targets = [target(7, [0.0, 0.0], [64.0, 64.0], false)];
        let mut router = InputRouter::new();
        assert_eq!(
            router.pointer_moved(&targets, [1.0, 1.0]),
            PointerRouting::Widget(WidgetId(7)),
            "a corner far from the icon's own 4x4 footprint is still inside \
             its oversized hit region"
        );
    }

    #[test]
    fn a_press_captures_and_every_event_until_release_goes_to_the_same_widget() {
        let targets = [target(1, [0.0, 0.0], [16.0, 16.0], true)];
        let mut router = InputRouter::new();

        assert_eq!(
            router.pointer_button(&targets, [8.0, 8.0], PointerButton::Primary, true),
            PointerRouting::Widget(WidgetId(1))
        );
        assert_eq!(router.captured(), Some(WidgetId(1)));

        assert_eq!(
            router.pointer_moved(&targets, [500.0, 500.0]),
            PointerRouting::Widget(WidgetId(1)),
            "capture must hold even once the pointer has left the region"
        );
        assert_eq!(
            router.pointer_button(&targets, [500.0, 500.0], PointerButton::Primary, true),
            PointerRouting::Widget(WidgetId(1)),
            "a second press while captured still belongs to the captor"
        );

        assert_eq!(
            router.pointer_button(&targets, [500.0, 500.0], PointerButton::Primary, false),
            PointerRouting::Widget(WidgetId(1)),
            "the release itself is still reported to the widget that held capture"
        );
        assert_eq!(router.captured(), None, "release must clear capture");

        assert_eq!(
            router.pointer_moved(&targets, [500.0, 500.0]),
            PointerRouting::World
        );
    }

    #[test]
    fn a_different_button_releasing_does_not_end_capture_only_the_one_that_started_it_does() {
        let targets = [target(1, [0.0, 0.0], [16.0, 16.0], true)];
        let mut router = InputRouter::new();

        router.pointer_button(&targets, [8.0, 8.0], PointerButton::Primary, true);
        assert_eq!(router.captured(), Some(WidgetId(1)));

        assert_eq!(
            router.pointer_button(&targets, [500.0, 500.0], PointerButton::Secondary, true),
            PointerRouting::Widget(WidgetId(1))
        );
        assert_eq!(
            router.pointer_button(&targets, [500.0, 500.0], PointerButton::Secondary, false),
            PointerRouting::Widget(WidgetId(1))
        );
        assert_eq!(
            router.captured(),
            Some(WidgetId(1)),
            "a different button releasing must not end the capture"
        );

        router.pointer_button(&targets, [500.0, 500.0], PointerButton::Primary, false);
        assert_eq!(
            router.captured(),
            None,
            "the button that started the capture releasing must end it"
        );
    }

    #[test]
    fn a_press_on_a_focusable_widget_focuses_it_and_a_press_on_the_world_does_not_change_focus() {
        let targets = [
            target(1, [0.0, 0.0], [16.0, 16.0], true),
            target(2, [16.0, 0.0], [32.0, 16.0], false),
        ];
        let mut router = InputRouter::new();

        router.pointer_button(&targets, [8.0, 8.0], PointerButton::Primary, true);
        router.pointer_button(&targets, [8.0, 8.0], PointerButton::Primary, false);
        assert_eq!(router.focused(), Some(WidgetId(1)));

        router.pointer_button(&targets, [24.0, 8.0], PointerButton::Primary, true);
        router.pointer_button(&targets, [24.0, 8.0], PointerButton::Primary, false);
        assert_eq!(
            router.focused(),
            Some(WidgetId(1)),
            "a non-focusable widget must not take focus"
        );

        router.pointer_button(&targets, [999.0, 999.0], PointerButton::Primary, true);
        assert_eq!(
            router.focused(),
            Some(WidgetId(1)),
            "R9 names a focusable click and a Tab traversal, not a world click"
        );
    }

    #[test]
    fn tab_traversal_cycles_focusable_targets_and_wraps_both_ways() {
        let targets = [
            target(1, [0.0, 0.0], [1.0, 1.0], true),
            target(2, [1.0, 0.0], [2.0, 1.0], false),
            target(3, [2.0, 0.0], [3.0, 1.0], true),
        ];
        let mut router = InputRouter::new();

        router.focus_next(&targets);
        assert_eq!(router.focused(), Some(WidgetId(1)));
        router.focus_next(&targets);
        assert_eq!(
            router.focused(),
            Some(WidgetId(3)),
            "the non-focusable target in between must be skipped"
        );
        router.focus_next(&targets);
        assert_eq!(router.focused(), Some(WidgetId(1)), "must wrap forward");

        router.focus_previous(&targets);
        assert_eq!(router.focused(), Some(WidgetId(3)), "must wrap backward");
    }

    #[test]
    fn release_capture_and_clear_focus_are_available_as_an_explicit_escape_hatch() {
        let targets = [target(1, [0.0, 0.0], [16.0, 16.0], true)];
        let mut router = InputRouter::new();
        router.pointer_button(&targets, [8.0, 8.0], PointerButton::Primary, true);
        assert_eq!(router.captured(), Some(WidgetId(1)));
        assert_eq!(router.focused(), Some(WidgetId(1)));

        router.release_capture();
        assert_eq!(router.captured(), None);
        router.clear_focus();
        assert_eq!(router.focused(), None);
    }

    #[test]
    fn hovering_updates_regardless_of_which_widget_holds_capture() {
        let targets = [
            target(1, [0.0, 0.0], [16.0, 16.0], false),
            target(2, [16.0, 0.0], [32.0, 16.0], false),
        ];
        let mut router = InputRouter::new();
        router.pointer_button(&targets, [8.0, 8.0], PointerButton::Primary, true);
        assert_eq!(router.captured(), Some(WidgetId(1)));

        router.pointer_moved(&targets, [24.0, 8.0]);
        assert_eq!(
            router.hovered(),
            Some(WidgetId(2)),
            "hover is a function of pointer position, independent of capture"
        );
        assert_eq!(router.captured(), Some(WidgetId(1)));
    }
}
