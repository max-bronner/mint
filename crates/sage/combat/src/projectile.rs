use mint_sim::StateHasher;

use crate::geom::Point3;

const LENGTH_TOLERANCE: f32 = 1.0;

const MAX_LENGTH_DEPTH: u32 = 8;

const TERRAIN_SAMPLE_SPACING: f32 = 10.0;

const TRACK_DEAD_ZONE_SQUARED: f32 = 0.1;

const MAX_TERRAIN_SAMPLES: u32 = 512;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BezierArc {
    pub first_height: f32,
    pub second_height: f32,
    pub first_percent_indent: f32,
    pub second_percent_indent: f32,
    pub ignore_terrain_height: bool,
    pub first_percent_height: f32,
    pub second_percent_height: f32,
    pub curve_flatten_min_dist: f32,
}

impl Default for BezierArc {
    fn default() -> Self {
        Self {
            first_height: 0.0,
            second_height: 0.0,
            first_percent_indent: 0.0,
            second_percent_indent: 1.0,
            ignore_terrain_height: false,
            first_percent_height: DEFAULT_FIRST_PERCENT_HEIGHT,
            second_percent_height: DEFAULT_SECOND_PERCENT_HEIGHT,
            curve_flatten_min_dist: 0.0,
        }
    }
}

pub const DEFAULT_FIRST_PERCENT_HEIGHT: f32 = 0.33;

pub const DEFAULT_SECOND_PERCENT_HEIGHT: f32 = 0.66;

impl BezierArc {
    #[must_use]
    pub fn control_points(
        &self,
        start: Point3,
        end: Point3,
        highest_terrain: Option<f32>,
    ) -> [Point3; 4] {
        if !start.is_finite() || !end.is_finite() {
            return [start; 4];
        }
        let first = clamp01(self.first_percent_indent);
        let second = clamp01(self.second_percent_indent);
        let finite = |value: f32| if value.is_finite() { value } else { 0.0 };
        let terrain = highest_terrain.filter(|value| value.is_finite());
        let point = |indent: f32, z: f32| {
            let along = start.lerp(end, indent);
            Point3::new(along.x, along.y, z)
        };
        if self.ignore_terrain_height {
            let rise = end.z - start.z;
            return [
                start,
                point(first, start.z + rise * finite(self.first_percent_height)),
                point(second, start.z + rise * finite(self.second_percent_height)),
                end,
            ];
        }
        if let Some(scale) = self.flatten_scale(start, end) {
            let above_chord = |indent: f32, height: f32| {
                let chord = start.lerp(end, indent).z;
                let floor = match terrain {
                    Some(ground) if ground > chord => ground,
                    _ => chord,
                };
                point(indent, floor + finite(height) * scale)
            };
            return [
                start,
                above_chord(first, self.first_height),
                above_chord(second, self.second_height),
                end,
            ];
        }
        let mut base = if start.z > end.z { start.z } else { end.z };
        if let Some(ground) = terrain {
            if ground > base {
                base = ground;
            }
        }
        [
            start,
            point(first, base + finite(self.first_height)),
            point(second, base + finite(self.second_height)),
            end,
        ]
    }

    fn flatten_scale(&self, start: Point3, end: Point3) -> Option<f32> {
        let min_dist = self.curve_flatten_min_dist;
        if !min_dist.is_finite() || min_dist <= 0.0 {
            return None;
        }
        let half = min_dist * 0.5;
        let distance = start.distance_squared_3d(end).sqrt();
        Some(if !distance.is_finite() || distance < half {
            0.0
        } else if distance < min_dist {
            (distance - half) / half
        } else {
            1.0
        })
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.first_height);
        hasher.write_f32(self.second_height);
        hasher.write_f32(self.first_percent_indent);
        hasher.write_f32(self.second_percent_indent);
        hasher.write_bool(self.ignore_terrain_height);
        hasher.write_f32(self.first_percent_height);
        hasher.write_f32(self.second_percent_height);
        hasher.write_f32(self.curve_flatten_min_dist);
    }
}

#[must_use]
pub fn highest_terrain_along(
    start: Point3,
    end: Point3,
    height_at: impl Fn(f32, f32) -> Option<f32>,
) -> Option<f32> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let distance = start.distance_squared_2d(end).sqrt();
    if !distance.is_finite() {
        return None;
    }
    let steps = (distance / TERRAIN_SAMPLE_SPACING).ceil();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = if steps.is_finite() && steps > 1.0 {
        (steps as u32).min(MAX_TERRAIN_SAMPLES)
    } else {
        1
    };
    let mut highest: Option<f32> = None;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let at = start.lerp(end, t);
        if let Some(height) = height_at(at.x, at.y).filter(|value| value.is_finite()) {
            highest = Some(match highest {
                Some(best) if best >= height => best,
                _ => height,
            });
        }
    }
    highest
}

fn clamp01(t: f32) -> f32 {
    if !t.is_finite() || t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    }
}

fn point_on(points: &[Point3; 4], t: f32) -> Point3 {
    let t = clamp01(t);
    let [p0, p1, p2, p3] = *points;
    let a = p0.lerp(p1, t);
    let b = p1.lerp(p2, t);
    let c = p2.lerp(p3, t);
    let d = a.lerp(b, t);
    let e = b.lerp(c, t);
    d.lerp(e, t)
}

#[must_use]
pub fn bezier_length(points: &[Point3; 4]) -> f32 {
    approximate_length(points, 0)
}

#[must_use]
pub fn bezier_points(points: &[Point3; 4], count: u32) -> Vec<Point3> {
    if count < 2 {
        return vec![points[0]];
    }
    let last = count.saturating_sub(1);
    #[allow(clippy::cast_precision_loss)]
    (0..count)
        .map(|i| point_on(points, i as f32 / last as f32))
        .collect()
}

fn approximate_length(points: &[Point3; 4], depth: u32) -> f32 {
    let [p0, p1, p2, p3] = *points;
    let leg = |a: Point3, b: Point3| a.distance_squared_3d(b).sqrt();
    let chord = leg(p0, p3);
    let polygon = leg(p0, p1) + leg(p1, p2) + leg(p2, p3);
    if !chord.is_finite() || !polygon.is_finite() {
        return 0.0;
    }
    if polygon - chord > LENGTH_TOLERANCE && depth < MAX_LENGTH_DEPTH {
        let (first, second) = split_at_half(points);
        let next = depth.saturating_add(1);
        return approximate_length(&first, next) + approximate_length(&second, next);
    }
    (chord + polygon) / 2.0
}

fn split_at_half(points: &[Point3; 4]) -> ([Point3; 4], [Point3; 4]) {
    let [p0, p1, p2, p3] = *points;
    let a = p0.lerp(p1, 0.5);
    let b = p1.lerp(p2, 0.5);
    let c = p2.lerp(p3, 0.5);
    let d = a.lerp(b, 0.5);
    let e = b.lerp(c, 0.5);
    let mid = d.lerp(e, 0.5);
    ([p0, a, d, mid], [mid, e, c, p3])
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectileFlight {
    points: [Point3; 4],
    frame: u16,
    frames: u16,
}

impl ProjectileFlight {
    #[must_use]
    pub fn launch(
        start: Point3,
        end: Point3,
        arc: BezierArc,
        highest_terrain: Option<f32>,
        speed: f32,
        hz: f32,
    ) -> Self {
        let points = arc.control_points(start, end, highest_terrain);
        let length = approximate_length(&points, 0);
        let frames = if speed.is_finite()
            && speed > 0.0
            && length.is_finite()
            && length > 0.0
            && hz.is_finite()
            && hz > 0.0
        {
            let per_frame = speed / hz;
            let count = (length / per_frame).ceil();
            if count.is_finite() && count > 1.0 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let count = count.min(f32::from(u16::MAX)) as u16;
                count
            } else {
                1
            }
        } else {
            1
        };
        Self {
            points,
            frame: 0,
            frames,
        }
    }

    #[must_use]
    pub fn through(points: [Point3; 4], frames: u16) -> Self {
        Self {
            points,
            frame: 0,
            frames: frames.max(1),
        }
    }

    #[must_use]
    pub fn ballistic(start: Point3, velocity: Point3, end: Point3, frames: u16) -> Self {
        let frames = frames.max(1);
        let half = f32::from(frames) * 0.5;
        let middle = Point3::new(
            start.x + velocity.x * half,
            start.y + velocity.y * half,
            start.z + velocity.z * half,
        );
        let raise = 2.0 / 3.0;
        Self {
            points: [
                start,
                start.lerp(middle, raise),
                end.lerp(middle, raise),
                end,
            ],
            frame: 0,
            frames,
        }
    }

    #[must_use]
    pub fn start(&self) -> Point3 {
        let [start, _, _, _] = self.points;
        start
    }

    #[must_use]
    pub fn end(&self) -> Point3 {
        let [_, _, _, end] = self.points;
        end
    }

    #[must_use]
    pub fn control_points(&self) -> [Point3; 4] {
        self.points
    }

    #[must_use]
    pub fn curve_length(&self) -> f32 {
        approximate_length(&self.points, 0)
    }

    #[must_use]
    pub fn t(&self) -> f32 {
        f32::from(self.frame) / f32::from(self.frames.max(1))
    }

    #[must_use]
    pub fn step(&self) -> f32 {
        1.0 / f32::from(self.frames.max(1))
    }

    #[must_use]
    pub const fn frames(&self) -> u16 {
        self.frames
    }

    pub fn track(
        &mut self,
        target: Point3,
        max_step: f32,
        arc: BezierArc,
        highest_terrain: impl FnOnce(Point3, Point3) -> Option<f32>,
    ) -> bool {
        let end = self.end();
        let moved_squared = end.distance_squared_3d(target);
        if !target.is_finite()
            || !max_step.is_finite()
            || max_step <= 0.0
            || !moved_squared.is_finite()
            || moved_squared <= TRACK_DEAD_ZONE_SQUARED
        {
            return false;
        }
        let moved = moved_squared.sqrt();
        let step = if moved > max_step { max_step } else { moved };
        let aim = end.lerp(target, step / moved);
        let start = self.start();
        self.points = arc.control_points(start, aim, highest_terrain(start, aim));
        true
    }

    pub fn advance(&mut self) -> bool {
        self.frame = self.frame.saturating_add(1).min(self.frames);
        self.frame >= self.frames
    }

    #[must_use]
    pub fn position(&self) -> Point3 {
        point_on(&self.points, self.t())
    }

    #[must_use]
    pub fn at(&self, t: f32) -> Point3 {
        point_on(&self.points, t)
    }

    #[must_use]
    pub fn ticks_to_arrival(&self) -> u32 {
        u32::from(self.frames.saturating_sub(self.frame).max(1))
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        for point in self.points {
            point.hash_into(hasher);
        }
        hasher.write_u16(self.frame);
        hasher.write_u16(self.frames);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
