#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }

    #[must_use]
    pub fn minus(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }

    #[must_use]
    pub fn scale(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k, self.z * k)
    }

    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[must_use]
    pub fn normalised(self) -> Option<Self> {
        let squared = self.length_squared();
        if !squared.is_finite() || squared <= f32::MIN_POSITIVE {
            return None;
        }
        Some(self.scale(1.0 / squared.sqrt()))
    }

    #[must_use]
    pub const fn horizontal(self) -> crate::Vec2 {
        crate::Vec2::new(self.x, self.y)
    }

    #[must_use]
    pub const fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

pub const SUBDIVISIONS: u16 = 16;

pub const MAX_POINTS: usize = 64;

#[must_use]
pub fn hermite(from: Vec3, leave: Vec3, to: Vec3, arrive: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = 3.0 * t2 - 2.0 * t3;
    let h11 = t3 - t2;
    from.scale(h00)
        .plus(leave.scale(h10))
        .plus(to.scale(h01))
        .plus(arrive.scale(h11))
}

fn weighted_tangent(before: Vec3, at: Vec3, after: Vec3, near: Vec3) -> Vec3 {
    let span = after.minus(before);
    let segment = near.minus(at).length();
    let total = at.minus(before).length() + after.minus(at).length();
    if total > f32::MIN_POSITIVE && total.is_finite() {
        span.scale(segment / total)
    } else {
        span.scale(0.5)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlightPath {
    points: Vec<Vec3>,
    marks: Vec<f32>,
}

impl FlightPath {
    #[must_use]
    pub fn new(points: Vec<Vec3>) -> Option<Self> {
        if points.len() < 4 || points.len() > MAX_POINTS || !points.iter().all(|p| p.is_finite()) {
            return None;
        }
        let mut marks = Vec::with_capacity(
            points
                .len()
                .checked_sub(3)?
                .checked_mul(usize::from(SUBDIVISIONS))?
                .checked_add(1)?,
        );
        marks.push(0.0);
        let mut travelled = 0.0f32;
        let mut path = Self { points, marks };
        for segment in 0..path.points.len().saturating_sub(3) {
            let (from, leave, to, arrive) = path.segment(segment)?;
            let mut previous = from;
            for step in 1..=SUBDIVISIONS {
                let t = f32::from(step) / f32::from(SUBDIVISIONS);
                let next = hermite(from, leave, to, arrive, t);
                travelled += next.minus(previous).length();
                path.marks.push(travelled);
                previous = next;
            }
        }
        if !travelled.is_finite() {
            return None;
        }
        Some(path)
    }

    fn segment(&self, segment: usize) -> Option<(Vec3, Vec3, Vec3, Vec3)> {
        let first = segment.checked_add(1)?;
        let last = self.points.len().checked_sub(2)?;
        let &[before, from, to, after] = self.points.get(segment..segment.checked_add(4)?)? else {
            return None;
        };
        let leave = if first == 1 {
            to.minus(before).scale(0.5)
        } else {
            weighted_tangent(before, from, to, to)
        };
        let arrive = if first.checked_add(1)? == last {
            after.minus(from).scale(0.5)
        } else {
            weighted_tangent(from, to, after, from)
        };
        Some((from, leave, to, arrive))
    }

    #[must_use]
    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    #[must_use]
    pub fn length(&self) -> f32 {
        self.marks.last().copied().unwrap_or(0.0)
    }

    #[must_use]
    pub fn start(&self) -> Vec3 {
        self.points.get(1).copied().unwrap_or_default()
    }

    #[must_use]
    pub fn end(&self) -> Vec3 {
        self.points
            .len()
            .checked_sub(2)
            .and_then(|at| self.points.get(at))
            .copied()
            .unwrap_or_default()
    }

    fn locate(&self, distance: f32) -> Option<(usize, f32)> {
        if !distance.is_finite() || distance < 0.0 || distance > self.length() {
            return None;
        }
        let chord = self
            .marks
            .partition_point(|mark| *mark <= distance)
            .checked_sub(1)?;
        let chords = self.marks.len().checked_sub(1)?;
        let chord = chord.min(chords.checked_sub(1)?);
        let from = *self.marks.get(chord)?;
        let to = *self.marks.get(chord.checked_add(1)?)?;
        let along = if to > from {
            ((distance - from) / (to - from)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let per = usize::from(SUBDIVISIONS);
        let segment = chord.checked_div(per)?;
        let within = u16::try_from(chord.checked_rem(per)?).ok()?;
        Some((
            segment,
            (f32::from(within) + along) / f32::from(SUBDIVISIONS),
        ))
    }

    #[must_use]
    pub fn sample(&self, distance: f32) -> Option<Vec3> {
        let (segment, t) = self.locate(distance)?;
        let (from, leave, to, arrive) = self.segment(segment)?;
        Some(hermite(from, leave, to, arrive, t))
    }

    #[must_use]
    pub fn segment_at(&self, distance: f32) -> Option<usize> {
        self.locate(distance).map(|(segment, _)| segment)
    }

    #[must_use]
    pub fn with_point_in(&self, segment: usize, point: Vec3) -> Option<Self> {
        let at = segment.checked_add(2)?;
        if at >= self.points.len().checked_sub(1)? {
            return None;
        }
        let mut points = self.points.clone();
        points.insert(at, point);
        Self::new(points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight() -> FlightPath {
        FlightPath::new(vec![
            Vec3::new(-10.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(20.0, 0.0, 0.0),
        ])
        .unwrap()
    }

    #[test]
    fn evenly_spaced_collinear_points_are_a_straight_path_walked_evenly() {
        let path = straight();
        assert!((path.length() - 10.0).abs() < 1e-4);
        for step in 0..=10u8 {
            let at = path.sample(f32::from(step)).unwrap();
            assert!((at.x - f32::from(step)).abs() < 1e-3, "{step}: {at:?}");
            assert_eq!(at.y, 0.0);
        }
        assert_eq!(path.sample(-0.1), None);
        assert_eq!(path.sample(10.1), None);
        assert_eq!(path.sample(f32::NAN), None);
    }

    #[test]
    fn a_path_passes_through_its_inner_points_and_leaves_along_its_first_tangent() {
        let behind = Vec3::new(-300.0, 0.0, 0.0);
        let here = Vec3::new(0.0, 0.0, 100.0);
        let there = Vec3::new(0.0, 200.0, 0.0);
        let beyond = Vec3::new(0.0, 300.0, 0.0);
        let path = FlightPath::new(vec![behind, here, there, beyond]).unwrap();
        assert_eq!(path.sample(0.0).unwrap(), here);
        let end = path.sample(path.length()).unwrap();
        assert!(end.minus(there).length() < 1e-3, "{end:?}");
        let early = path.sample(1.0).unwrap().minus(here);
        assert!(early.x > early.y && early.z < 0.0, "{early:?}");
    }

    #[test]
    fn a_sample_is_a_point_of_the_curve_and_distances_increase_along_it() {
        let path = FlightPath::new(vec![
            Vec3::new(-50.0, -50.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(40.0, 10.0, 5.0),
            Vec3::new(60.0, 60.0, 5.0),
            Vec3::new(100.0, 60.0, 0.0),
        ])
        .unwrap();
        let mut last = path.sample(0.0).unwrap();
        let steps = 200u16;
        for step in 1..=steps {
            let distance = (path.length() * f32::from(step) / f32::from(steps)).min(path.length());
            let at = path.sample(distance).unwrap();
            let moved = at.minus(last).length();
            let expected = path.length() / f32::from(steps);
            assert!(
                (moved - expected).abs() < 0.05 * expected,
                "step {step}: moved {moved}, expected {expected}"
            );
            last = at;
        }
    }

    fn uniform(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
        let (t2, t3) = (t * t, t * t * t);
        let axis = |a: f32, b: f32, c: f32, d: f32| {
            0.5 * (2.0 * b
                + (c - a) * t
                + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                + (3.0 * b - a - 3.0 * c + d) * t3)
        };
        Vec3::new(
            axis(p0.x, p1.x, p2.x, p3.x),
            axis(p0.y, p1.y, p2.y, p3.y),
            axis(p0.z, p1.z, p2.z, p3.z),
        )
    }

    #[test]
    fn a_four_point_path_and_an_evenly_spaced_one_are_the_uniform_spline() {
        let four = [
            Vec3::new(-300.0, 20.0, 0.0),
            Vec3::new(0.0, 0.0, 100.0),
            Vec3::new(50.0, 200.0, 0.0),
            Vec3::new(60.0, 300.0, -40.0),
        ];
        let path = FlightPath::new(four.to_vec()).unwrap();
        let (from, leave, to, arrive) = path.segment(0).unwrap();
        for step in 0..=10u8 {
            let t = f32::from(step) / 10.0;
            let ours = hermite(from, leave, to, arrive, t);
            let theirs = uniform(four[0], four[1], four[2], four[3], t);
            assert!(
                ours.minus(theirs).length() < 1e-3,
                "{t}: {ours:?} {theirs:?}"
            );
        }
        let even: Vec<Vec3> = (0..6u8)
            .map(|i| Vec3::new(f32::from(i) * 10.0, f32::from(i % 2) * 5.0, 0.0))
            .collect();
        let path = FlightPath::new(even.clone()).unwrap();
        let (from, leave, to, arrive) = path.segment(1).unwrap();
        let ours = hermite(from, leave, to, arrive, 0.3);
        let theirs = uniform(even[1], even[2], even[3], even[4], 0.3);
        assert!(ours.minus(theirs).length() < 1e-3, "{ours:?} {theirs:?}");
    }

    #[test]
    fn bunched_points_between_long_legs_never_double_the_path_back() {
        let path = FlightPath::new(vec![
            Vec3::new(-391.0, 0.0, 60.0),
            Vec3::new(200.0, 0.0, 60.0),
            Vec3::new(397.0, 0.0, 117.0),
            Vec3::new(428.0, 0.0, 124.0),
            Vec3::new(445.0, 0.0, 124.0),
            Vec3::new(750.0, 0.0, 60.0),
            Vec3::new(887.0, 0.0, 60.0),
        ])
        .unwrap();
        let mut last = path.sample(0.0).unwrap().x;
        let steps = 400u16;
        for step in 1..=steps {
            let distance = (path.length() * f32::from(step) / f32::from(steps)).min(path.length());
            let x = path.sample(distance).unwrap().x;
            assert!(
                x >= last - 1e-3,
                "ran west from {last} to {x} at {distance}"
            );
            last = x;
        }
    }

    #[test]
    fn a_point_added_inside_a_segment_bends_the_path_through_it() {
        let lifted = Vec3::new(5.0, 0.0, 20.0);
        let path = straight().with_point_in(0, lifted).unwrap();
        assert_eq!(path.points().len(), 5);
        assert_eq!(path.points()[2], lifted);
        assert_eq!(path.start(), Vec3::ZERO);
        assert_eq!(path.end(), Vec3::new(10.0, 0.0, 0.0));
        assert!(path.length() > 10.0);
        assert_eq!(straight().with_point_in(1, lifted), None);
    }

    #[test]
    fn too_few_too_many_or_non_finite_points_are_no_path() {
        assert_eq!(FlightPath::new(vec![Vec3::ZERO; 3]), None);
        assert_eq!(FlightPath::new(vec![Vec3::ZERO; MAX_POINTS + 1]), None);
        let mut points = vec![Vec3::ZERO; 4];
        points[2].z = f32::INFINITY;
        assert_eq!(FlightPath::new(points), None);
        let still = FlightPath::new(vec![Vec3::ZERO; 4]).unwrap();
        assert_eq!(still.length(), 0.0);
        assert_eq!(still.sample(0.0), Some(Vec3::ZERO));
    }
}
