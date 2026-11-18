use super::Route;
use crate::geom::Vec2;
use crate::grid::Portal;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct OnPath {
    pub(super) point: Vec2,
    pub(super) waypoint: u32,
}

fn index(waypoint: u32) -> usize {
    usize::try_from(waypoint).unwrap_or(usize::MAX)
}

fn waypoint_of(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

pub(super) fn at_portal(route: &Route, waypoint: u32) -> bool {
    let at = index(waypoint);
    [at, at.saturating_add(1)]
        .iter()
        .any(|&i| route.portals.get(i).copied().flatten().is_some())
}

pub(super) fn walk(route: &Route, from: Vec2, waypoint: u32, distance: f32) -> OnPath {
    let mut at = from;
    let mut left = if distance.is_finite() {
        distance.max(0.0)
    } else {
        0.0
    };
    let first = index(waypoint);
    for (i, point) in route.points.iter().enumerate().skip(first) {
        let leg = point.minus(at);
        let length = leg.length();
        if length >= left && length > 0.0 {
            return OnPath {
                point: at.plus(leg.scale(left / length)),
                waypoint: waypoint_of(i),
            };
        }
        left -= length;
        at = *point;
    }
    OnPath {
        point: at,
        waypoint: waypoint_of(route.points.len().saturating_sub(1).max(first)),
    }
}

pub(super) fn length_left(
    route: &Route,
    from: Vec2,
    waypoint: u32,
    measure: impl Fn(Vec2) -> f32,
) -> f32 {
    let mut at = from;
    let mut left = 0.0;
    for point in route.points.iter().skip(index(waypoint)) {
        left += measure(point.minus(at));
        at = *point;
    }
    left
}

pub(super) fn octagonal(leg: Vec2) -> f32 {
    let (x, y) = (leg.x.abs(), leg.y.abs());
    x.max(y) + x.min(y) / 4.0
}

pub(super) fn cursor(route: &Route, position: Vec2, waypoint: u32) -> u32 {
    let count = route.points.len();
    let mut at = index(waypoint);
    while at.saturating_add(1) < count {
        let (Some(previous), Some(point)) = (
            at.checked_sub(1).and_then(|i| route.points.get(i)),
            route.points.get(at),
        ) else {
            break;
        };
        if position.minus(*point).dot(point.minus(*previous)) < 0.0 {
            break;
        }
        at = at.saturating_add(1);
    }
    waypoint_of(at)
}

const WIDE_TURN: f32 = 0.23 * core::f32::consts::PI;
const SHARPEST_ROUNDED: f32 = 0.55 * core::f32::consts::PI;
const ARC_STEP_RADIUS: f32 = 30.0;
const MAX_ARC_STEPS: f32 = 128.0;

pub fn round_corners(route: &mut Route, radius: f32, clear: impl Fn(Vec2) -> bool) {
    if !radius.is_finite() || radius <= 0.0 || route.points.len() < 3 {
        return;
    }
    let mut out: Vec<Vec2> = Vec::with_capacity(route.points.len());
    let mut tags: Vec<Option<Portal>> = Vec::with_capacity(route.points.len());
    let mut points = route.points.iter().copied();
    let mut portals = route.portals.iter().copied();
    let Some(first) = points.next() else {
        return;
    };
    out.push(first);
    tags.push(portals.next().flatten());
    let mut corner = match points.next() {
        Some(point) => point,
        None => return,
    };
    let mut corner_tag = portals.next().flatten();
    for next in points {
        let next_tag = portals.next().flatten();
        let previous = out.last().copied().unwrap_or(first);
        let arc = if corner_tag.is_some() {
            None
        } else {
            fillet(previous, corner, next, radius)
        };
        match arc {
            Some(arc) if arc.iter().all(|point| clear(*point)) => {
                tags.extend(std::iter::repeat_n(None, arc.len()));
                out.extend(arc);
            }
            _ => {
                out.push(corner);
                tags.push(corner_tag);
            }
        }
        corner = next;
        corner_tag = next_tag;
    }
    out.push(corner);
    tags.push(corner_tag);
    route.points = out;
    route.portals = tags;
}

fn fillet(from: Vec2, corner: Vec2, to: Vec2, radius: f32) -> Option<Vec<Vec2>> {
    let leg_in = corner.minus(from);
    let leg_out = to.minus(corner);
    let (length_in, length_out) = (leg_in.length(), leg_out.length());
    let dir_in = leg_in.normalised()?;
    let dir_out = leg_out.normalised()?;
    let angle = mint_math::f32::acos(dir_in.dot(dir_out).clamp(-1.0, 1.0));
    if !angle.is_finite() || angle <= 0.0 || angle > SHARPEST_ROUNDED {
        return None;
    }
    let mut r = radius.min(0.75 * length_in.min(length_out));
    if angle < WIDE_TURN {
        r *= 1.75;
    }
    let steps = ((r / ARC_STEP_RADIUS) * angle / (core::f32::consts::PI / 16.0))
        .ceil()
        .clamp(1.0, MAX_ARC_STEPS);
    if steps < 2.0 || !steps.is_finite() {
        return None;
    }
    let tangent = r * mint_math::f32::tan(angle / 2.0);
    if !tangent.is_finite() || tangent > length_in || tangent > length_out {
        return None;
    }
    let side = if dir_in.x * dir_out.y - dir_in.y * dir_out.x >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let start = corner.minus(dir_in.scale(tangent));
    let centre = start.plus(Vec2::new(-dir_in.y * side, dir_in.x * side).scale(r));
    let spoke = start.minus(centre);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = steps as u32;
    let mut arc = Vec::with_capacity(usize::try_from(count).unwrap_or(0).saturating_add(1));
    for step in 0..=count {
        #[allow(clippy::cast_precision_loss)]
        let turned = side * angle * step as f32 / steps;
        let (sin, cos) = mint_math::f32::sin_cos(turned);
        arc.push(centre.plus(Vec2::new(
            spoke.x * cos - spoke.y * sin,
            spoke.x * sin + spoke.y * cos,
        )));
    }
    Some(arc)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Departure {
    pub heading: crate::geom::Facing,
    pub radius: f32,
    pub max_turn: f32,
}

const DEPART_STRAIGHT_BELOW: f32 = core::f32::consts::PI / 16.0;

const DEPART_RETRY_RADIUS: f32 = 5.0;

pub fn depart(route: &mut Route, departure: Departure, clear: impl Fn(Vec2) -> bool) {
    let (Some(&start), Some(&first)) = (route.points.first(), route.points.get(1)) else {
        return;
    };
    if !departure.radius.is_finite() || departure.radius <= 0.0 {
        return;
    }
    let leg = first.minus(start);
    let Some(dir) = leg.normalised() else {
        return;
    };
    let heading = departure.heading.to_vector();
    let angle = mint_math::f32::acos(heading.dot(dir).clamp(-1.0, 1.0));
    if !angle.is_finite() || angle < DEPART_STRAIGHT_BELOW || angle > departure.max_turn {
        return;
    }
    let side = if heading.x * dir.y - heading.y * dir.x > 0.0 {
        1.0
    } else {
        -1.0
    };
    let half_leg = 0.5 * leg.length();
    for radius in [departure.radius, DEPART_RETRY_RADIUS] {
        let r = radius.min(half_leg);
        if let Some(arc) = departure_arc(start, heading, side, r, first) {
            if arc.iter().all(|point| clear(*point)) {
                let at = 1.min(route.portals.len());
                route
                    .portals
                    .splice(at..at, std::iter::repeat_n(None, arc.len()));
                route.points.splice(1..1, arc);
                return;
            }
        }
    }
}

fn departure_arc(start: Vec2, heading: Vec2, side: f32, r: f32, first: Vec2) -> Option<Vec<Vec2>> {
    if !r.is_finite() || r <= 0.0 {
        return None;
    }
    let centre = start.plus(Vec2::new(-heading.y * side, heading.x * side).scale(r));
    let to_first = first.minus(centre).normalised()?;
    let end = Vec2::new(to_first.y * side, -to_first.x * side);
    let spoke = start.minus(centre).scale(1.0 / r);
    let turned = mint_math::f32::atan2(
        spoke.x * end.y - spoke.y * end.x,
        spoke.x * end.x + spoke.y * end.y,
    ) * side;
    let sweep = if turned < 0.0 {
        turned + 2.0 * core::f32::consts::PI
    } else {
        turned
    };
    let square_point = centre.plus(end.scale(r));
    let top_up = first.minus(square_point).normalised().map_or(0.0, |d| {
        let d_rot = Vec2::new(d.y * side, -d.x * side);
        mint_math::f32::acos(d_rot.dot(end).clamp(-1.0, 1.0))
    });
    let th = sweep + top_up;
    let th = if th > 1.5 * core::f32::consts::PI {
        th - 2.0 * core::f32::consts::PI
    } else {
        th
    };
    let th = th.max(0.0);
    let steps = ((r / ARC_STEP_RADIUS) * th / (core::f32::consts::PI / 16.0))
        .ceil()
        .clamp(1.0, MAX_ARC_STEPS);
    if !steps.is_finite() || !th.is_finite() || th <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = steps as u32;
    if count < 2 {
        return Some(Vec::new());
    }
    let mut arc = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for step in 1..=count {
        #[allow(clippy::cast_precision_loss)]
        let (sin, cos) = mint_math::f32::sin_cos(side * th * step as f32 / steps);
        arc.push(centre.plus(
            Vec2::new(spoke.x * cos - spoke.y * sin, spoke.x * sin + spoke.y * cos).scale(r),
        ));
    }
    Some(arc)
}
