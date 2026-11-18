use crate::input::{HitTarget, PointerRouting, Rect, WidgetId};

#[derive(Debug, Clone, Copy)]
pub struct CommandBar<'a> {
    pub origin: [f32; 2],
    pub columns: usize,
    pub slot_size: [f32; 2],
    pub gap: f32,
    pub slot_ids: &'a [WidgetId],
}

impl<'a> CommandBar<'a> {
    #[must_use]
    pub fn new(
        origin: [f32; 2],
        columns: usize,
        slot_size: [f32; 2],
        slot_ids: &'a [WidgetId],
    ) -> Self {
        Self {
            origin,
            columns,
            slot_size,
            gap: 0.0,
            slot_ids,
        }
    }

    #[must_use]
    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slot_ids.len()
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn slot_rect(&self, index: usize) -> Option<Rect> {
        if self.columns == 0 || index >= self.slot_ids.len() {
            return None;
        }
        let row = index / self.columns;
        let col = index % self.columns;
        let stride = [self.slot_size[0] + self.gap, self.slot_size[1] + self.gap];
        let min = [
            self.origin[0] + col as f32 * stride[0],
            self.origin[1] + row as f32 * stride[1],
        ];
        Some(Rect {
            min,
            max: [min[0] + self.slot_size[0], min[1] + self.slot_size[1]],
        })
    }

    #[must_use]
    pub fn hit_targets(&self) -> Vec<HitTarget> {
        self.slot_ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                self.slot_rect(index).map(|region| HitTarget {
                    id: *id,
                    region,
                    focusable: true,
                })
            })
            .collect()
    }

    #[must_use]
    pub fn slot_for_pointer_button(&self, routing: PointerRouting, pressed: bool) -> Option<usize> {
        if pressed {
            return None;
        }
        let PointerRouting::Widget(id) = routing else {
            return None;
        };
        self.slot_ids.iter().position(|candidate| *candidate == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputRouter, PointerButton};

    fn ids(n: u64) -> Vec<WidgetId> {
        (0..n).map(WidgetId).collect()
    }

    #[test]
    fn slots_lay_out_row_major_with_the_configured_gap() {
        let slot_ids = ids(6);
        let bar = CommandBar::new([100.0, 200.0], 3, [20.0, 20.0], &slot_ids).with_gap(4.0);

        assert_eq!(
            bar.slot_rect(0),
            Some(Rect {
                min: [100.0, 200.0],
                max: [120.0, 220.0]
            })
        );
        assert_eq!(
            bar.slot_rect(1),
            Some(Rect {
                min: [124.0, 200.0],
                max: [144.0, 220.0]
            }),
            "second column offsets by slot width plus gap"
        );
        assert_eq!(
            bar.slot_rect(3),
            Some(Rect {
                min: [100.0, 224.0],
                max: [120.0, 244.0]
            }),
            "fourth slot wraps to the second row's first column"
        );
        assert_eq!(bar.slot_rect(6), None, "past the end of slot_ids");
    }

    #[test]
    fn zero_columns_never_divides_by_zero_and_reports_no_rects() {
        let slot_ids = ids(3);
        let bar = CommandBar::new([0.0, 0.0], 0, [10.0, 10.0], &slot_ids);
        assert_eq!(bar.slot_rect(0), None);
        assert!(bar.hit_targets().is_empty());
    }

    #[test]
    fn each_slot_click_reports_its_own_distinct_index() {
        let slot_ids = ids(4);
        let bar = CommandBar::new([0.0, 0.0], 2, [10.0, 10.0], &slot_ids);
        let targets = bar.hit_targets();
        let mut router = InputRouter::new();

        let mut clicked = Vec::new();
        for point in [[5.0, 5.0], [15.0, 5.0], [5.0, 15.0], [15.0, 15.0]] {
            router.pointer_button(&targets, point, PointerButton::Primary, true);
            let release = router.pointer_button(&targets, point, PointerButton::Primary, false);
            clicked.push(bar.slot_for_pointer_button(release, false));
        }
        assert_eq!(
            clicked,
            vec![Some(0), Some(1), Some(2), Some(3)],
            "each of the four points landed in its own distinct slot"
        );
    }

    #[test]
    fn a_slot_click_does_not_reach_the_world_and_a_click_elsewhere_does() {
        let slot_ids = ids(2);
        let bar = CommandBar::new([0.0, 0.0], 2, [16.0, 16.0], &slot_ids);
        let targets = bar.hit_targets();
        let mut router = InputRouter::new();
        let mut world_events = 0usize;
        let mut slot_events = 0usize;

        for point in [[5.0, 5.0], [500.0, 500.0], [20.0, 5.0]] {
            router.pointer_button(&targets, point, PointerButton::Primary, true);
            let release = router.pointer_button(&targets, point, PointerButton::Primary, false);
            match release {
                PointerRouting::Widget(_) => {
                    assert!(
                        bar.slot_for_pointer_button(release, false).is_some(),
                        "a widget hit inside this test must be one of the bar's own slots"
                    );
                    slot_events += 1;
                }
                PointerRouting::World => world_events += 1,
            }
        }

        assert_eq!(slot_events, 2, "the two points inside a slot");
        assert_eq!(world_events, 1, "the point far outside the whole grid");
    }

    #[test]
    fn a_press_never_reports_a_slot_only_a_release_does() {
        let slot_ids = ids(1);
        let bar = CommandBar::new([0.0, 0.0], 1, [10.0, 10.0], &slot_ids);
        let routing = PointerRouting::Widget(WidgetId(0));

        assert_eq!(bar.slot_for_pointer_button(routing, true), None);
        assert_eq!(bar.slot_for_pointer_button(routing, false), Some(0));
    }

    #[test]
    fn an_unrecognised_id_reports_no_slot() {
        let slot_ids = ids(2);
        let bar = CommandBar::new([0.0, 0.0], 2, [10.0, 10.0], &slot_ids);
        let routing = PointerRouting::Widget(WidgetId(999));
        assert_eq!(bar.slot_for_pointer_button(routing, false), None);
    }
}
