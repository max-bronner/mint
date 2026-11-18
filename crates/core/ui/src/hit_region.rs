use crate::input::{HitTarget, Rect, WidgetId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitRegion {
    pub id: WidgetId,
    pub rect: Rect,
    pub focusable: bool,
}

impl HitRegion {
    #[must_use]
    pub fn new(id: WidgetId, rect: Rect) -> Self {
        Self {
            id,
            rect,
            focusable: false,
        }
    }

    #[must_use]
    pub fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }

    #[must_use]
    pub fn hit_target(&self) -> HitTarget {
        HitTarget {
            id: self.id,
            region: self.rect,
            focusable: self.focusable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputRouter, PointerButton, PointerRouting};

    fn rect(min: [f32; 2], max: [f32; 2]) -> Rect {
        Rect { min, max }
    }

    #[test]
    fn a_hit_region_produces_a_hit_test_result_indistinguishable_from_a_drawn_widgets() {
        let region = HitRegion::new(WidgetId(9), rect([0.0, 0.0], [16.0, 16.0]));
        let targets = [region.hit_target()];
        let mut router = InputRouter::new();

        assert_eq!(
            router.pointer_moved(&targets, [8.0, 8.0]),
            PointerRouting::Widget(WidgetId(9)),
            "hovering an undrawn region still routes to it"
        );
        assert_eq!(
            router.pointer_button(&targets, [8.0, 8.0], PointerButton::Primary, true),
            PointerRouting::Widget(WidgetId(9)),
            "clicking an undrawn region still produces a hit-test result"
        );
        assert_eq!(
            router.captured(),
            Some(WidgetId(9)),
            "capture works on a region with nothing drawn, the same as any widget"
        );
    }

    #[test]
    fn a_hit_region_is_not_focusable_unless_asked() {
        let plain = HitRegion::new(WidgetId(1), rect([0.0, 0.0], [1.0, 1.0]));
        assert!(!plain.hit_target().focusable);

        let reachable = plain.focusable();
        assert!(reachable.hit_target().focusable);
    }
}
