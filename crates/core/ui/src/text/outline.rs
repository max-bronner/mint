use skrifa::outline::OutlinePen;

use crate::text::error::{Error, Result};
use crate::text::font::GlyphId;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlyphOutline {
    pub contours: Vec<Vec<[f32; 2]>>,
}

pub const MAX_OUTLINE_POINTS: usize = 20_000;

const MAX_FLATTEN_DEPTH: u32 = 6;

const FLATTEN_TOLERANCE: f32 = 0.2;

pub(crate) struct ContourPen {
    contours: Vec<Vec<[f32; 2]>>,
    current: Vec<[f32; 2]>,
    start: [f32; 2],
    last: [f32; 2],
    total_points: usize,
    overflowed: bool,
}

impl ContourPen {
    pub(crate) fn new() -> Self {
        Self {
            contours: Vec::new(),
            current: Vec::new(),
            start: [0.0, 0.0],
            last: [0.0, 0.0],
            total_points: 0,
            overflowed: false,
        }
    }

    fn push(&mut self, point: [f32; 2]) {
        if self.total_points >= MAX_OUTLINE_POINTS {
            self.overflowed = true;
            return;
        }
        self.current.push(point);
        self.total_points += 1;
        self.last = point;
    }

    fn close_current(&mut self) {
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
    }

    pub(crate) fn finish(mut self, glyph: GlyphId) -> Result<GlyphOutline> {
        self.close_current();
        if self.overflowed {
            return Err(Error::OutlineTooComplex(glyph, MAX_OUTLINE_POINTS));
        }
        Ok(GlyphOutline {
            contours: self.contours,
        })
    }
}

impl OutlinePen for ContourPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_current();
        self.start = [x, y];
        self.last = [x, y];
        self.push([x, y]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push([x, y]);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let p0 = self.last;
        flatten_quad(p0, [cx0, cy0], [x, y], MAX_FLATTEN_DEPTH, &mut |p| {
            self.push(p);
        });
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let p0 = self.last;
        flatten_cubic(
            p0,
            [cx0, cy0],
            [cx1, cy1],
            [x, y],
            MAX_FLATTEN_DEPTH,
            &mut |p| self.push(p),
        );
    }

    fn close(&mut self) {
        if self.last != self.start {
            self.push(self.start);
        }
        self.close_current();
    }
}

fn midpoint(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn distance_to_line(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len_sq = dx * dx + dy * dy;
    if len_sq <= f32::EPSILON {
        let ex = p[0] - a[0];
        let ey = p[1] - a[1];
        return (ex * ex + ey * ey).sqrt();
    }
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len_sq.sqrt()
}

fn flatten_quad(
    p0: [f32; 2],
    c: [f32; 2],
    p1: [f32; 2],
    depth: u32,
    sink: &mut impl FnMut([f32; 2]),
) {
    if depth == 0 || distance_to_line(c, p0, p1) <= FLATTEN_TOLERANCE {
        sink(p1);
        return;
    }
    let p01 = midpoint(p0, c);
    let p12 = midpoint(c, p1);
    let p012 = midpoint(p01, p12);
    flatten_quad(p0, p01, p012, depth - 1, &mut *sink);
    flatten_quad(p012, p12, p1, depth - 1, &mut *sink);
}

fn flatten_cubic(
    p0: [f32; 2],
    c0: [f32; 2],
    c1: [f32; 2],
    p1: [f32; 2],
    depth: u32,
    sink: &mut impl FnMut([f32; 2]),
) {
    let flat = distance_to_line(c0, p0, p1) <= FLATTEN_TOLERANCE
        && distance_to_line(c1, p0, p1) <= FLATTEN_TOLERANCE;
    if depth == 0 || flat {
        sink(p1);
        return;
    }
    let p01 = midpoint(p0, c0);
    let p12 = midpoint(c0, c1);
    let p23 = midpoint(c1, p1);
    let p012 = midpoint(p01, p12);
    let p123 = midpoint(p12, p23);
    let p0123 = midpoint(p012, p123);
    flatten_cubic(p0, p01, p012, p0123, depth - 1, &mut *sink);
    flatten_cubic(p0123, p123, p23, p1, depth - 1, &mut *sink);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_triangle_produces_one_closed_contour() {
        let mut pen = ContourPen::new();
        pen.move_to(0.0, 0.0);
        pen.line_to(10.0, 0.0);
        pen.line_to(5.0, 10.0);
        pen.close();
        let outline = pen.finish(GlyphId(0)).unwrap();
        assert_eq!(outline.contours.len(), 1);
        assert_eq!(
            outline.contours[0],
            vec![[0.0, 0.0], [10.0, 0.0], [5.0, 10.0], [0.0, 0.0]],
            "close() must append the start point when the last point isn't already it"
        );
    }

    #[test]
    fn a_second_move_to_starts_a_new_contour_even_without_a_close() {
        let mut pen = ContourPen::new();
        pen.move_to(0.0, 0.0);
        pen.line_to(1.0, 0.0);
        pen.move_to(5.0, 5.0);
        pen.line_to(6.0, 5.0);
        pen.close();
        let outline = pen.finish(GlyphId(0)).unwrap();
        assert_eq!(outline.contours.len(), 2);
        assert_eq!(outline.contours[0], vec![[0.0, 0.0], [1.0, 0.0]]);
    }

    #[test]
    fn a_perfectly_flat_quad_curve_emits_only_its_end_point() {
        let mut points = Vec::new();
        flatten_quad(
            [0.0, 0.0],
            [5.0, 0.0],
            [10.0, 0.0],
            MAX_FLATTEN_DEPTH,
            &mut |p| points.push(p),
        );
        assert_eq!(points, vec![[10.0, 0.0]]);
    }

    #[test]
    fn a_curved_quad_subdivides_and_stays_within_the_control_polygon() {
        let mut points = Vec::new();
        flatten_quad(
            [0.0, 0.0],
            [10.0, 20.0],
            [20.0, 0.0],
            MAX_FLATTEN_DEPTH,
            &mut |p| points.push(p),
        );
        assert!(
            points.len() > 1,
            "a curve this far from its chord must subdivide"
        );
        for [x, y] in &points {
            assert!((0.0..=20.0).contains(x), "x={x} outside the hull");
            assert!((0.0..=20.0).contains(y), "y={y} outside the hull");
        }
    }

    #[test]
    fn depth_zero_never_subdivides_however_curved() {
        let mut points = Vec::new();
        flatten_quad([0.0, 0.0], [1000.0, 1000.0], [20.0, 0.0], 0, &mut |p| {
            points.push(p)
        });
        assert_eq!(points, vec![[20.0, 0.0]]);
    }

    #[test]
    fn exceeding_the_point_cap_is_an_error_not_unbounded_growth() {
        let mut pen = ContourPen::new();
        pen.move_to(0.0, 0.0);
        for i in 0..MAX_OUTLINE_POINTS + 10 {
            #[allow(clippy::cast_precision_loss)]
            pen.line_to(i as f32, 0.0);
        }
        pen.close();
        let err = pen.finish(GlyphId(0)).unwrap_err();
        assert!(matches!(
            err,
            Error::OutlineTooComplex(GlyphId(0), MAX_OUTLINE_POINTS)
        ));
    }
}
