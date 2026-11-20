use mint_sage_ini::living_world::effects::ColorIntensityPoint;

pub const CLIENT_HZ: f32 = 30.0;

const DEFAULT_EASE: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    frame: u32,
    value: f32,
    ease_in: f32,
    ease_out: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Curve {
    points: Vec<Point>,
}

fn quadratic(a: f32, b: f32, c: f32, u: f32) -> f32 {
    let v = 1.0 - u;
    v * v * a + 2.0 * v * u * b + u * u * c
}

fn bezier(c1: f32, c2: f32, t: f32) -> f32 {
    let v = 1.0 - t;
    3.0 * v * v * t * c1 + 3.0 * v * t * t * c2 + t * t * t
}

fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    0.5 * (2.0 * p1
        + (p2 - p0) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t)
}

impl Curve {
    #[must_use]
    pub fn new(points: &[ColorIntensityPoint]) -> Self {
        Self {
            points: points
                .iter()
                .map(|p| Point {
                    frame: u32::try_from(
                        u64::from(p.time.map_or(0, |t| t.0)).saturating_mul(30) / 1000,
                    )
                    .unwrap_or(u32::MAX),
                    value: p.intensity.unwrap_or(1.0),
                    ease_in: p.ease_in.unwrap_or(DEFAULT_EASE),
                    ease_out: p.ease_out.unwrap_or(DEFAULT_EASE),
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    #[must_use]
    pub fn ended(&self, frame: u32) -> bool {
        self.points.last().is_some_and(|last| frame > last.frame)
    }

    #[must_use]
    pub fn value(&self, frame: u32) -> f32 {
        let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else {
            return 1.0;
        };
        if frame <= first.frame {
            return first.value;
        }
        if frame > last.frame {
            return last.value;
        }
        let Some(index) = self.points.iter().position(|p| frame <= p.frame) else {
            return last.value;
        };
        let (Some(p1), Some(p2)) = (
            self.points.get(index.saturating_sub(1)),
            self.points.get(index),
        ) else {
            return last.value;
        };
        let p0 = self
            .points
            .get(index.saturating_sub(2))
            .filter(|_| index >= 2)
            .unwrap_or(p1);
        let p3 = self.points.get(index.saturating_add(1)).unwrap_or(p2);
        let span = p2.frame.saturating_sub(p1.frame);
        if span == 0 {
            return p2.value;
        }
        let offset = f64::from(frame.saturating_sub(p1.frame));
        #[allow(clippy::cast_possible_truncation)]
        let t = (offset / f64::from(span)) as f32;
        let c1 = quadratic(0.0, 5.0 / 12.0, 0.5, 1.0 - p1.ease_out);
        let c2 = quadratic(0.5, 7.0 / 12.0, 1.0, p2.ease_in);
        catmull_rom(p0.value, p1.value, p2.value, p3.value, bezier(c1, c2, t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_ini::living_world::Millis;

    fn point(seconds_ms: u32, intensity: f32, ease: Option<(f32, f32)>) -> ColorIntensityPoint {
        ColorIntensityPoint {
            intensity: Some(intensity),
            time: Some(Millis(seconds_ms)),
            ease_in: ease.map(|e| e.0),
            ease_out: ease.map(|e| e.1),
            ..ColorIntensityPoint::default()
        }
    }

    #[test]
    fn times_become_truncated_client_frames_and_the_ends_hold() {
        let curve = Curve::new(&[point(0, 0.2, None), point(1000, 1.0, None)]);
        assert_eq!(curve.points[1].frame, 30);
        let curve = Curve::new(&[point(0, 0.2, None), point(333, 1.0, None)]);
        assert_eq!(curve.points[1].frame, 9);
        assert_eq!(curve.value(0), 0.2);
        assert_eq!(curve.value(500), 1.0);
        assert!(!curve.ended(9));
        assert!(curve.ended(10));
    }

    #[test]
    fn a_segment_between_interior_points_is_catmull_rom_not_linear() {
        let curve = Curve::new(&[
            point(0, 0.0, None),
            point(1000, 0.0, None),
            point(2000, 1.0, None),
            point(3000, 0.0, None),
        ]);
        let mid = curve.value(45);
        assert!((mid - 0.5).abs() > 0.05, "{mid}");
        assert!((mid - 0.5625).abs() < 1.0e-6, "{mid}");
        assert_eq!(curve.value(60), 1.0);
    }

    #[test]
    fn the_ease_fields_remap_the_segment_time() {
        let eased = Curve::new(&[
            point(0, 0.0, Some((0.5, 1.0))),
            point(1000, 1.0, Some((0.0, 0.5))),
        ]);
        let plain = Curve::new(&[point(0, 0.0, None), point(1000, 1.0, None)]);
        assert!((plain.value(15) - 0.5).abs() < 1.0e-6);
        assert!((plain.value(10) - catmull_rom(0.0, 0.0, 1.0, 1.0, 1.0 / 3.0)).abs() < 1.0e-6);
        let remapped = catmull_rom(0.0, 0.0, 1.0, 1.0, 0.3125);
        assert!((eased.value(15) - remapped).abs() < 1.0e-6);
    }
}
