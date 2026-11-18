use std::f32::consts::TAU;

pub(crate) const SEGMENT_TEXTURE: &str = "TerrainClaimSegment";

const SAMPLES: u16 = 640;

const SEGMENTS_PER_TURN: f32 = 32.0;

const MIN_SEGMENT: f32 = 20.0;
const MAX_SEGMENT: f32 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClaimCircle {
    pub(crate) centre: [f32; 2],
    pub(crate) radius: f32,
    pub(crate) colour: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Segment {
    pub(crate) centre: [f32; 2],
    pub(crate) side: f32,
    pub(crate) facing: f32,
    pub(crate) colour: [f32; 3],
}

pub(crate) fn outline(circles: &[ClaimCircle]) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (index, circle) in circles.iter().enumerate() {
        for (start, stop) in open_arcs(circles, index) {
            arc_segments(circle, start, stop, &mut segments);
        }
    }
    segments
}

fn covered(circles: &[ClaimCircle], index: usize, angle: f32) -> bool {
    let Some(circle) = circles.get(index) else {
        return false;
    };
    let x = circle.centre[0] + mint_math::f32::cos(angle) * circle.radius;
    let y = circle.centre[1] + mint_math::f32::sin(angle) * circle.radius;
    circles.iter().enumerate().any(|(other, c)| {
        let (dx, dy) = (x - c.centre[0], y - c.centre[1]);
        other != index && (dx * dx + dy * dy).sqrt() <= c.radius
    })
}

fn open_arcs(circles: &[ClaimCircle], index: usize) -> Vec<(f32, f32)> {
    let step = TAU / f32::from(SAMPLES);
    let angle = |k: u16| f32::from(k) * step;
    let mut seen_cover = false;
    let mut first_open = None;
    for k in 0..=SAMPLES {
        let cover = covered(circles, index, angle(k));
        if cover {
            seen_cover = true;
        } else if seen_cover {
            first_open = Some(angle(k));
            break;
        }
    }
    let Some(begin) = first_open else {
        return vec![(0.0, TAU)];
    };
    let mut arcs = Vec::new();
    let mut open_from = Some(begin);
    for k in 0..=SAMPLES + 1 {
        let at = begin + angle(k);
        let cover = covered(circles, index, at);
        match (open_from, cover) {
            (Some(from), true) => {
                arcs.push((from, at));
                open_from = None;
            }
            (None, false) => open_from = Some(at),
            _ => {}
        }
    }
    arcs
}

fn arc_segments(circle: &ClaimCircle, start: f32, stop: f32, out: &mut Vec<Segment>) {
    let sweep = stop - start;
    let length = sweep * circle.radius;
    let count = trunc(sweep / TAU * SEGMENTS_PER_TURN)
        .max(trunc(length / MAX_SEGMENT))
        .min(trunc(length / MIN_SEGMENT));
    if count == 0 {
        return;
    }
    let side = length / f32::from(count);
    let step = sweep / f32::from(count);
    let reach = circle.radius - side / 2.0;
    for k in 0..count {
        let mid = start + (f32::from(k) + 0.5) * step;
        let (s, c) = (mint_math::f32::sin(mid), mint_math::f32::cos(mid));
        out.push(Segment {
            centre: [circle.centre[0] + c * reach, circle.centre[1] + s * reach],
            side,
            facing: mid + TAU / 4.0,
            colour: circle.colour,
        });
    }
}

fn trunc(value: f32) -> u16 {
    if value.is_finite() && value >= 1.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = value.min(f32::from(u16::MAX)) as u16;
        count
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(x: f32, radius: f32) -> ClaimCircle {
        ClaimCircle {
            centre: [x, 0.0],
            radius,
            colour: [1.0; 3],
        }
    }

    fn angle_of(segment: &Segment, circle: &ClaimCircle) -> f32 {
        mint_math::f32::atan2(
            segment.centre[1] - circle.centre[1],
            segment.centre[0] - circle.centre[0],
        )
    }

    #[test]
    fn a_lone_circle_is_cut_into_its_share_of_segments() {
        let lone = outline(&[circle(0.0, 200.0)]);
        assert_eq!(lone.len(), 32);
        assert!((lone[0].side - TAU * 200.0 / 32.0).abs() < 1e-3);
        let [x, y] = lone[0].centre;
        let reach = (x * x + y * y).sqrt();
        assert!((reach - (200.0 - lone[0].side / 2.0)).abs() < 1e-3);
        assert_eq!(outline(&[circle(0.0, 50.0)]).len(), 15);
        assert_eq!(outline(&[circle(0.0, 600.0)]).len(), 62);
    }

    #[test]
    fn overlapping_circles_draw_only_the_outer_boundary_of_their_union() {
        let pair = [circle(0.0, 100.0), circle(150.0, 100.0)];
        let segments = outline(&pair);
        assert!(!segments.is_empty());
        for segment in &segments {
            let owner = if segment.centre[0] < 75.0 { 0 } else { 1 };
            let at = angle_of(segment, &pair[owner]);
            let other = &pair[1 - owner];
            let x = pair[owner].centre[0] + mint_math::f32::cos(at) * pair[owner].radius;
            let y = mint_math::f32::sin(at) * pair[owner].radius;
            let dx = x - other.centre[0];
            let inside = (dx * dx + y * y).sqrt() <= other.radius;
            assert!(!inside, "a segment at {at} inside the other circle");
        }
        let left = segments.iter().filter(|s| s.centre[0] < 75.0).count();
        assert_eq!(left, segments.len() - left);
        assert_eq!(left, 24);
    }

    #[test]
    fn a_circle_inside_another_is_drawn_whole() {
        let nested = outline(&[circle(0.0, 300.0), circle(10.0, 100.0)]);
        assert_eq!(nested.len(), 32 + 31);
    }
}
