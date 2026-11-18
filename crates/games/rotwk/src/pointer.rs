use crate::pick::ScreenRect;
use mint_app::PointerButton;
use std::time::Duration;

pub(crate) const DRAG_TOLERANCE_PIXELS: f32 = 15.0;

const CHORD_TOLERANCE_PIXELS: f32 = 5.0;

const CHORD_WINDOW: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Intent {
    Select { x: f32, y: f32 },
    SelectBox(ScreenRect),
    Order { x: f32, y: f32 },
    OrderDrag { anchor: (f32, f32), at: (f32, f32) },
    FormationDrag { anchor: (f32, f32), at: (f32, f32) },
    CameraReset,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Press {
    anchor: (f32, f32),
    dragging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pending {
    button: PointerButton,
    anchor: (f32, f32),
    at: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Pointer {
    at: Option<(f32, f32)>,
    primary: Option<Press>,
    secondary: Option<Press>,
    middle: Option<Press>,
    pending: Option<Pending>,
    chord_anchor: Option<(f32, f32)>,
}

impl Pointer {
    pub(crate) const fn position(&self) -> Option<(f32, f32)> {
        self.at
    }

    pub(crate) fn drag_rect(&self) -> Option<ScreenRect> {
        let press = self.primary?;
        if !press.dragging {
            return None;
        }
        Some(ScreenRect::from_corners(press.anchor, self.at?))
    }

    pub(crate) const fn rotating(&self) -> bool {
        self.middle.is_some()
    }

    pub(crate) const fn chord_drag_anchor(&self) -> Option<(f32, f32)> {
        self.chord_anchor
    }

    pub(crate) fn secondary_drag_anchor(&self) -> Option<(f32, f32)> {
        let press = self.secondary?;
        press.dragging.then_some(press.anchor)
    }

    pub(crate) fn scroll_axes(&self) -> Option<(f32, f32)> {
        let press = self.secondary?;
        if !press.dragging {
            return None;
        }
        let at = self.at?;
        let (dx, dy) = (at.0 - press.anchor.0, at.1 - press.anchor.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length <= 0.0 {
            return None;
        }
        Some((-dy / length, dx / length))
    }

    pub(crate) fn moved(&mut self, x: f32, y: f32) {
        self.at = Some((x, y));
        for press in [&mut self.primary, &mut self.secondary, &mut self.middle] {
            if let Some(press) = press.as_mut() {
                if !press.dragging && past_tolerance(press.anchor, (x, y)) {
                    press.dragging = true;
                }
            }
        }
    }

    pub(crate) fn button(
        &mut self,
        button: PointerButton,
        pressed: bool,
        now: Duration,
    ) -> Option<Intent> {
        let at = self.at?;
        match (button, pressed) {
            (PointerButton::Middle, true) => {
                self.middle = Some(Press {
                    anchor: at,
                    dragging: false,
                });
                None
            }
            (PointerButton::Middle, false) => {
                let press = self.middle.take()?;
                (!press.dragging).then_some(Intent::CameraReset)
            }
            (PointerButton::Primary, true) => {
                self.primary = Some(Press {
                    anchor: at,
                    dragging: false,
                });
                self.chord_press(button, at, now);
                None
            }
            (PointerButton::Primary, false) => {
                if let Some(intent) = self.chord_release(button, at) {
                    return Some(intent);
                }
                let press = self.primary.take()?;
                if press.dragging {
                    Some(Intent::SelectBox(ScreenRect::from_corners(
                        press.anchor,
                        at,
                    )))
                } else {
                    Some(Intent::Select { x: at.0, y: at.1 })
                }
            }
            (PointerButton::Secondary, true) => {
                self.secondary = Some(Press {
                    anchor: at,
                    dragging: false,
                });
                self.chord_press(button, at, now);
                None
            }
            (PointerButton::Secondary, false) => {
                if let Some(intent) = self.chord_release(button, at) {
                    return Some(intent);
                }
                let press = self.secondary.take()?;
                if press.dragging {
                    Some(Intent::OrderDrag {
                        anchor: press.anchor,
                        at,
                    })
                } else {
                    Some(Intent::Order { x: at.0, y: at.1 })
                }
            }
            _ => None,
        }
    }

    fn chord_press(&mut self, button: PointerButton, at: (f32, f32), now: Duration) {
        match self.pending.take() {
            Some(pending) if pending.button != button => {
                let (dx, dy) = (at.0 - pending.anchor.0, at.1 - pending.anchor.1);
                if dx.abs() < CHORD_TOLERANCE_PIXELS
                    && dy.abs() < CHORD_TOLERANCE_PIXELS
                    && now.saturating_sub(pending.at) < CHORD_WINDOW
                {
                    self.primary = None;
                    self.secondary = None;
                    self.chord_anchor = Some(pending.anchor);
                }
            }
            _ => {
                self.pending = Some(Pending {
                    button,
                    anchor: at,
                    at: now,
                })
            }
        }
    }

    fn chord_release(&mut self, button: PointerButton, at: (f32, f32)) -> Option<Intent> {
        if let Some(pending) = self.pending {
            if pending.button == button {
                self.pending = None;
            }
        }
        let anchor = self.chord_anchor.take()?;
        Some(Intent::FormationDrag { anchor, at })
    }

    pub(crate) fn cancel(&mut self) {
        self.primary = None;
        self.secondary = None;
        self.middle = None;
        self.pending = None;
        self.chord_anchor = None;
    }
}

fn past_tolerance(anchor: (f32, f32), now: (f32, f32)) -> bool {
    let dx = now.0 - anchor.0;
    let dy = now.1 - anchor.1;
    dx * dx + dy * dy > DRAG_TOLERANCE_PIXELS * DRAG_TOLERANCE_PIXELS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RadarArmed {
    None,
    AttackMove,
    Cast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RadarAction {
    LookAt,
    Move,
    AttackMove,
    Cast,
}

pub(crate) fn radar_click(
    button: PointerButton,
    pressed: bool,
    selection_empty: bool,
    alternate_mouse_setup: bool,
    armed: RadarArmed,
) -> Option<RadarAction> {
    if !pressed {
        return None;
    }
    if selection_empty {
        return Some(RadarAction::LookAt);
    }
    let look_button = if alternate_mouse_setup {
        PointerButton::Primary
    } else {
        PointerButton::Secondary
    };
    if button == look_button {
        return Some(RadarAction::LookAt);
    }
    match button {
        PointerButton::Primary | PointerButton::Secondary => Some(match armed {
            RadarArmed::Cast => RadarAction::Cast,
            RadarArmed::AttackMove => RadarAction::AttackMove,
            RadarArmed::None => RadarAction::Move,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: Duration = Duration::ZERO;

    fn pointer_at(x: f32, y: f32) -> Pointer {
        let mut pointer = Pointer::default();
        pointer.moved(x, y);
        pointer
    }

    #[test]
    fn a_press_and_release_in_the_same_place_is_a_click() {
        let mut pointer = pointer_at(100.0, 100.0);
        assert_eq!(pointer.button(PointerButton::Primary, true, T0), None);
        assert_eq!(
            pointer.button(PointerButton::Primary, false, T0),
            Some(Intent::Select { x: 100.0, y: 100.0 })
        );
    }

    #[test]
    fn a_press_that_wanders_inside_the_tolerance_is_still_a_click() {
        let mut pointer = pointer_at(100.0, 100.0);
        pointer.button(PointerButton::Primary, true, T0);
        pointer.moved(105.0, 108.0);
        assert!(pointer.drag_rect().is_none());
        assert!(matches!(
            pointer.button(PointerButton::Primary, false, T0),
            Some(Intent::Select { .. })
        ));
    }

    #[test]
    fn a_press_that_travels_past_the_tolerance_is_a_box() {
        let mut pointer = pointer_at(100.0, 100.0);
        pointer.button(PointerButton::Primary, true, T0);
        pointer.moved(140.0, 160.0);
        let rect = pointer.drag_rect().unwrap();
        assert_eq!(
            rect,
            ScreenRect::from_corners((100.0, 100.0), (140.0, 160.0))
        );
        assert_eq!(
            pointer.button(PointerButton::Primary, false, T0),
            Some(Intent::SelectBox(rect))
        );
        assert!(
            pointer.drag_rect().is_none(),
            "the drag ends with the press"
        );
    }

    #[test]
    fn a_drag_that_returns_to_its_anchor_is_still_a_drag() {
        let mut pointer = pointer_at(100.0, 100.0);
        pointer.button(PointerButton::Primary, true, T0);
        pointer.moved(300.0, 300.0);
        pointer.moved(100.0, 100.0);
        assert!(matches!(
            pointer.button(PointerButton::Primary, false, T0),
            Some(Intent::SelectBox(_))
        ));
    }

    #[test]
    fn a_right_click_orders_and_a_right_drag_gives_the_drag_endpoints() {
        let mut pointer = pointer_at(200.0, 200.0);
        pointer.button(PointerButton::Secondary, true, T0);
        assert_eq!(
            pointer.button(PointerButton::Secondary, false, T0),
            Some(Intent::Order { x: 200.0, y: 200.0 })
        );

        pointer.button(PointerButton::Secondary, true, T0);
        pointer.moved(400.0, 260.0);
        assert_eq!(
            pointer.button(PointerButton::Secondary, false, T0),
            Some(Intent::OrderDrag {
                anchor: (200.0, 200.0),
                at: (400.0, 260.0),
            })
        );
    }

    #[test]
    fn a_box_swept_backwards_is_the_same_rectangle() {
        let mut back = pointer_at(400.0, 400.0);
        back.button(PointerButton::Primary, true, T0);
        back.moved(100.0, 100.0);
        let Some(Intent::SelectBox(swept)) = back.button(PointerButton::Primary, false, T0) else {
            panic!("expected a box");
        };
        assert_eq!(swept.min_x, 100.0);
        assert_eq!(swept.max_y, 400.0);
    }

    #[test]
    fn a_button_event_before_the_pointer_has_ever_moved_is_ignored() {
        let mut pointer = Pointer::default();
        assert_eq!(pointer.button(PointerButton::Primary, true, T0), None);
        assert_eq!(pointer.button(PointerButton::Primary, false, T0), None);
        assert!(pointer.drag_rect().is_none());
    }

    #[test]
    fn cancelling_drops_a_press_in_progress() {
        let mut pointer = pointer_at(100.0, 100.0);
        pointer.button(PointerButton::Primary, true, T0);
        pointer.moved(300.0, 300.0);
        assert!(pointer.drag_rect().is_some());
        pointer.cancel();
        assert!(pointer.drag_rect().is_none());
        assert_eq!(pointer.button(PointerButton::Primary, false, T0), None);
    }

    #[test]
    fn a_close_fast_second_press_chords_into_a_formation_drag() {
        let mut pointer = pointer_at(600.0, 400.0);
        pointer.button(PointerButton::Primary, true, T0);
        pointer.moved(603.0, 403.0);
        pointer.button(PointerButton::Secondary, true, Duration::from_millis(200));
        assert!(pointer.drag_rect().is_none());
        pointer.moved(800.0, 400.0);
        assert_eq!(
            pointer.button(PointerButton::Secondary, false, T0),
            Some(Intent::FormationDrag {
                anchor: (600.0, 400.0),
                at: (800.0, 400.0),
            })
        );
        assert_eq!(pointer.button(PointerButton::Primary, false, T0), None);
    }

    #[test]
    fn a_second_press_past_the_pixel_tolerance_does_not_chord() {
        let mut pointer = pointer_at(600.0, 400.0);
        pointer.button(PointerButton::Primary, true, T0);
        pointer.moved(610.0, 400.0);
        pointer.button(PointerButton::Secondary, true, Duration::from_millis(50));
        assert_eq!(
            pointer.button(PointerButton::Secondary, false, T0),
            Some(Intent::Order { x: 610.0, y: 400.0 })
        );
        assert_eq!(
            pointer.button(PointerButton::Primary, false, T0),
            Some(Intent::Select { x: 610.0, y: 400.0 })
        );
    }

    #[test]
    fn a_second_press_past_the_time_window_does_not_chord() {
        let mut pointer = pointer_at(600.0, 400.0);
        pointer.button(PointerButton::Primary, true, T0);
        pointer.button(PointerButton::Secondary, true, Duration::from_millis(500));
        assert_eq!(
            pointer.button(PointerButton::Secondary, false, T0),
            Some(Intent::Order { x: 600.0, y: 400.0 })
        );
    }

    #[test]
    fn releasing_the_first_button_early_drops_the_pending_chord() {
        let mut pointer = pointer_at(600.0, 400.0);
        pointer.button(PointerButton::Primary, true, T0);
        pointer.button(PointerButton::Primary, false, T0);
        pointer.button(PointerButton::Secondary, true, Duration::from_millis(10));
        assert_eq!(
            pointer.button(PointerButton::Secondary, false, T0),
            Some(Intent::Order { x: 600.0, y: 400.0 })
        );
    }

    #[test]
    fn a_middle_click_resets_and_a_middle_drag_does_not() {
        let mut pointer = pointer_at(500.0, 300.0);
        pointer.button(PointerButton::Middle, true, T0);
        assert!(pointer.rotating());
        assert_eq!(
            pointer.button(PointerButton::Middle, false, T0),
            Some(Intent::CameraReset)
        );
        assert!(!pointer.rotating());

        pointer.button(PointerButton::Middle, true, T0);
        pointer.moved(540.0, 300.0);
        assert_eq!(pointer.button(PointerButton::Middle, false, T0), None);
    }

    #[test]
    fn a_plain_right_drag_scrolls_towards_the_pointer() {
        let mut pointer = pointer_at(400.0, 300.0);
        pointer.button(PointerButton::Secondary, true, T0);
        assert!(
            pointer.scroll_axes().is_none(),
            "not past the drag tolerance yet"
        );
        pointer.moved(400.0, 280.0);
        let (forward, right) = pointer.scroll_axes().expect("dragging up the screen");
        assert!(forward > 0.0, "up the screen is forward");
        assert!(right.abs() < 1e-6);
        pointer.button(PointerButton::Secondary, false, T0);
        assert!(pointer.scroll_axes().is_none());
    }

    #[test]
    fn radar_click_resolves_by_selection_then_button_then_armed() {
        let cases = [
            (
                PointerButton::Primary,
                true,
                RadarArmed::None,
                Some(RadarAction::LookAt),
            ),
            (
                PointerButton::Secondary,
                true,
                RadarArmed::None,
                Some(RadarAction::LookAt),
            ),
            (
                PointerButton::Primary,
                false,
                RadarArmed::None,
                Some(RadarAction::LookAt),
            ),
            (
                PointerButton::Secondary,
                false,
                RadarArmed::None,
                Some(RadarAction::Move),
            ),
            (
                PointerButton::Secondary,
                false,
                RadarArmed::AttackMove,
                Some(RadarAction::AttackMove),
            ),
            (
                PointerButton::Secondary,
                false,
                RadarArmed::Cast,
                Some(RadarAction::Cast),
            ),
            (
                PointerButton::Primary,
                false,
                RadarArmed::Cast,
                Some(RadarAction::LookAt),
            ),
            (PointerButton::Middle, false, RadarArmed::None, None),
        ];
        for (button, selection_empty, armed, want) in cases {
            assert_eq!(
                radar_click(button, true, selection_empty, true, armed),
                want,
                "button={button:?} selection_empty={selection_empty} armed={armed:?}"
            );
        }
    }

    #[test]
    fn radar_click_ignores_the_release() {
        assert_eq!(
            radar_click(
                PointerButton::Secondary,
                false,
                false,
                true,
                RadarArmed::None
            ),
            None
        );
    }

    #[test]
    fn radar_click_follows_alternate_mouse_setup_off() {
        assert_eq!(
            radar_click(
                PointerButton::Secondary,
                true,
                false,
                false,
                RadarArmed::None
            ),
            Some(RadarAction::LookAt)
        );
        assert_eq!(
            radar_click(PointerButton::Primary, true, false, false, RadarArmed::None),
            Some(RadarAction::Move)
        );
    }
}
