const SEARCH_STEP: f32 = 30.0;
const SEARCH_RINGS: u32 = 17;

const JUMP_DISTANCE: f32 = 100.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AreaRestriction {
    area: Vec<(f32, f32)>,
    last_valid: [f32; 2],
}

impl AreaRestriction {
    pub(crate) fn new(area: Vec<(f32, f32)>, at: [f32; 2]) -> Self {
        let last_valid = if contains(&area, at) {
            at
        } else {
            area.first().map_or(at, |&(x, y)| [x, y])
        };
        Self { area, last_valid }
    }

    pub(crate) fn clamp(
        &mut self,
        point: [f32; 2],
        shrouded: impl Fn([f32; 2]) -> bool,
    ) -> [f32; 2] {
        let valid = |p: [f32; 2]| !shrouded(p) && contains(&self.area, p);
        let last = self.last_valid;
        let (dx, dy) = (point[0] - last[0], point[1] - last[1]);
        let apart = (dx * dx + dy * dy).sqrt();
        if apart > JUMP_DISTANCE {
            for ring in 0..SEARCH_RINGS {
                #[allow(clippy::cast_precision_loss)]
                let reach = ring as f32 * SEARCH_STEP;
                for (i, j) in NEIGHBOURS {
                    let candidate = [point[0] + i * reach, point[1] + j * reach];
                    if valid(candidate) {
                        self.last_valid = candidate;
                        return candidate;
                    }
                }
            }
        }
        if !shrouded(last) && shrouded(point) {
            return last;
        }
        if contains(&self.area, point) {
            self.last_valid = point;
            return point;
        }
        let out = exit_point(&self.area, last, point).unwrap_or(last);
        self.last_valid = out;
        out
    }
}

const NEIGHBOURS: [(f32, f32); 8] = [
    (-1.0, -1.0),
    (-1.0, 0.0),
    (-1.0, 1.0),
    (0.0, -1.0),
    (0.0, 1.0),
    (1.0, -1.0),
    (1.0, 0.0),
    (1.0, 1.0),
];

fn contains(polygon: &[(f32, f32)], p: [f32; 2]) -> bool {
    let mut inside = false;
    let mut previous = match polygon.last() {
        Some(&v) => v,
        None => return false,
    };
    for &(x, y) in polygon {
        if (y > p[1]) != (previous.1 > p[1])
            && p[0] < (previous.0 - x) * (p[1] - y) / (previous.1 - y) + x
        {
            inside = !inside;
        }
        previous = (x, y);
    }
    inside
}

fn exit_point(polygon: &[(f32, f32)], from: [f32; 2], to: [f32; 2]) -> Option<[f32; 2]> {
    let mut best: Option<f32> = None;
    let mut previous = *polygon.last()?;
    for &(x, y) in polygon {
        let (ex, ey) = (x - previous.0, y - previous.1);
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let denominator = dx * ey - dy * ex;
        if denominator != 0.0 {
            let (fx, fy) = (previous.0 - from[0], previous.1 - from[1]);
            let t = (fx * ey - fy * ex) / denominator;
            let u = (fx * dy - fy * dx) / denominator;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                best = Some(best.map_or(t, |b| b.min(t)));
            }
        }
        previous = (x, y);
    }
    best.map(|t| {
        [
            from[0] + (to[0] - from[0]) * t,
            from[1] + (to[1] - from[1]) * t,
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<(f32, f32)> {
        vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
    }

    #[test]
    fn a_scroll_out_of_the_area_stops_on_its_edge() {
        let mut area = AreaRestriction::new(square(), [50.0, 50.0]);
        let stopped = area.clamp([140.0, 50.0], |_| false);
        assert!((stopped[0] - 100.0).abs() < 1e-3 && (stopped[1] - 50.0).abs() < 1e-3);
    }

    #[test]
    fn a_scroll_into_the_shroud_is_refused() {
        let mut area = AreaRestriction::new(square(), [50.0, 50.0]);
        let refused = area.clamp([70.0, 50.0], |p| p[0] > 60.0);
        assert_eq!(refused, [50.0, 50.0]);
    }
}
