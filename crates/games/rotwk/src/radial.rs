use mint_render::{Camera, Vec3};

use crate::command_bar::ResolvedSlot;

pub(crate) const BUTTON_STAGE: f32 = 48.0;
pub(crate) const RING_SPACING_STAGE: f32 = 56.0;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RadialPage {
    owner: Option<mint_sim::Entity>,
    stack: Vec<(u32, u32)>,
}

impl RadialPage {
    pub(crate) fn follow(&mut self, selected: Option<mint_sim::Entity>) {
        if self.owner != selected {
            self.owner = selected;
            self.stack.clear();
        }
    }

    pub(crate) fn push(&mut self, start: u32, count: u32) {
        self.stack.push((start, count));
    }

    pub(crate) fn pop(&mut self) {
        let _ = self.stack.pop();
    }

    pub(crate) fn current(&self) -> Option<(u32, u32)> {
        self.stack.last().copied()
    }
}

pub(crate) fn visible(
    buttons: &[(u32, ResolvedSlot)],
    root: Option<u32>,
    page: Option<(u32, u32)>,
) -> Vec<&ResolvedSlot> {
    let (first, last) = match page {
        Some((start, count)) => (start.saturating_add(1), start.saturating_add(count)),
        None => (1, root.unwrap_or(u32::MAX)),
    };
    buttons
        .iter()
        .filter(|(slot, _)| (first..=last).contains(slot))
        .map(|(_, button)| button)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RadialAnchor {
    pub(crate) centre: [f32; 2],
    pub(crate) footprint: f32,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn anchor(
    camera: &Camera,
    viewport: (u32, u32),
    position: [f32; 2],
    ground: f32,
    facing: f32,
    height: f32,
    radius: f32,
    menu_point: Option<[f32; 3]>,
) -> Option<RadialAnchor> {
    let (lx, ly, lz) = menu_point.map_or((0.0, 0.0, height * 0.5), |[x, y, z]| (x, y, z));
    let turn = mint_movement::Facing::from_radians(facing).to_vector();
    let (cos, sin) = (turn.x, turn.y);
    let world = Vec3::new(
        position[0] + lx * cos - ly * sin,
        position[1] + lx * sin + ly * cos,
        ground + lz,
    );
    let (x, y, _) = crate::pick::project(camera, viewport, world)?;
    let forward = camera.forward();
    let across = (forward.x * forward.x + forward.y * forward.y).sqrt();
    let (rx, ry) = if across > f32::EPSILON {
        (forward.y / across, -forward.x / across)
    } else {
        (1.0, 0.0)
    };
    let edge = Vec3::new(world.x + rx * radius, world.y + ry * radius, world.z);
    let footprint = crate::pick::project(camera, viewport, edge).map_or(0.0, |(ex, ey, _)| {
        ((ex - x) * (ex - x) + (ey - y) * (ey - y)).sqrt()
    });
    Some(RadialAnchor {
        centre: [x, y],
        footprint,
    })
}

pub(crate) fn layout(
    anchor: RadialAnchor,
    count: usize,
    size: f32,
    max_radius: f32,
) -> Vec<[f32; 4]> {
    if count == 0 || !size.is_finite() || size <= 0.0 {
        return Vec::new();
    }
    #[allow(clippy::cast_precision_loss)]
    let n = count as f32;
    let tau = core::f32::consts::TAU;
    let spacing = size * RING_SPACING_STAGE / BUTTON_STAGE;
    let radius = if count == 1 {
        0.0
    } else {
        spacing / (2.0 * mint_math::f32::sin(core::f32::consts::PI / n))
    }
    .min(max_radius.max(0.0));
    (0..count)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let angle = tau * i as f32 / n - core::f32::consts::FRAC_PI_2;
            let turn = mint_movement::Facing::from_radians(angle).to_vector();
            let cx = anchor.centre[0] + radius * turn.x;
            let cy = anchor.centre[1] + radius * turn.y;
            [cx - size * 0.5, cy - size * 0.5, size, size]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]

    use super::*;
    use crate::command_bar::SlotAction;

    fn slot(name: &str) -> ResolvedSlot {
        ResolvedSlot {
            action: SlotAction::Unimplemented("test"),
            label: name.to_owned(),
            command_button: name.to_owned(),
            enabled: true,
            availability: crate::command_bar::Availability::Enabled,
            image: None,
            cost: None,
            progress: None,
            count: 0,
            auto_cast: false,
        }
    }

    fn names(v: &[&ResolvedSlot]) -> Vec<String> {
        v.iter().map(|s| s.label.clone()).collect()
    }

    #[test]
    fn a_page_is_the_root_range_or_the_pushed_one() {
        let buttons: Vec<(u32, ResolvedSlot)> = [1, 2, 5, 7, 8, 13, 14, 15]
            .into_iter()
            .map(|n| (n, slot(&n.to_string())))
            .collect();
        assert_eq!(
            names(&visible(&buttons, Some(7), None)),
            ["1", "2", "5", "7"]
        );
        assert_eq!(
            names(&visible(&buttons, Some(7), Some((7, 7)))),
            ["8", "13", "14"]
        );
        assert_eq!(names(&visible(&buttons, None, None)).len(), 8, "no root");
    }

    #[test]
    fn the_page_stack_follows_the_selection() {
        let mut world = mint_sim::World::new();
        let (a, b) = (world.spawn().unwrap(), world.spawn().unwrap());
        let mut page = RadialPage::default();
        page.follow(Some(a));
        page.push(7, 7);
        page.follow(Some(a));
        assert_eq!(page.current(), Some((7, 7)), "same selection keeps it");
        page.pop();
        assert_eq!(page.current(), None);
        page.push(14, 10);
        page.follow(Some(b));
        assert_eq!(page.current(), None, "another selection resets");
    }

    fn centre(r: [f32; 4]) -> [f32; 2] {
        [r[0] + r[2] * 0.5, r[1] + r[3] * 0.5]
    }

    #[test]
    fn buttons_go_clockwise_from_the_top_on_one_circle() {
        let anchor = RadialAnchor {
            centre: [500.0, 400.0],
            footprint: 0.0,
        };
        let rects = layout(anchor, 4, 50.0, 1000.0);
        let c: Vec<[f32; 2]> = rects.iter().map(|&r| centre(r)).collect();
        assert!((c[0][0] - 500.0).abs() < 1e-3 && c[0][1] < 400.0, "top");
        assert!(c[1][0] > 500.0 && (c[1][1] - 400.0).abs() < 1e-3, "right");
        assert!(c[2][1] > 400.0, "bottom");
        let r0 = (c[0][1] - 400.0).abs();
        for p in &c {
            let r = ((p[0] - 500.0) * (p[0] - 500.0) + (p[1] - 400.0) * (p[1] - 400.0)).sqrt();
            assert!((r - r0).abs() < 1e-2);
        }
    }

    #[test]
    fn neighbours_keep_the_retail_spacing_and_the_radius_follows_the_count() {
        let anchor = RadialAnchor {
            centre: [0.0, 0.0],
            footprint: 300.0,
        };
        let button = 48.0 * 1.5625;
        let spacing = 56.0 * 1.5625;
        for (count, measured) in [(3usize, 51.7f32), (4, 62.0), (9, 127.8)] {
            let rects = layout(anchor, count, button, 1000.0);
            let c: Vec<[f32; 2]> = rects.iter().map(|&r| centre(r)).collect();
            let radius = (c[0][0] * c[0][0] + c[0][1] * c[0][1]).sqrt();
            assert!(
                (radius - measured).abs() < 1.6,
                "{count}: {radius} vs {measured}"
            );
            let (dx, dy) = (c[0][0] - c[1][0], c[0][1] - c[1][1]);
            let d = (dx * dx + dy * dy).sqrt();
            assert!(
                (d - spacing).abs() < 1e-2,
                "{count}: neighbours {d} px apart"
            );
        }
        let one = layout(anchor, 1, button, 1000.0);
        assert_eq!(centre(one[0]), [0.0, 0.0]);
        let capped = layout(anchor, 12, button, 100.0);
        assert!((centre(capped[0])[1].abs() - 100.0).abs() < 1e-3);
        assert!(layout(anchor, 0, button, 1000.0).is_empty());
    }
}
