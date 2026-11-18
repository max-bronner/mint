use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::geom::{Facing, Vec2};

pub const MAX_CANDIDATES: usize = 2048;

const MAX_BUCKET_SPAN: i32 = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footprint {
    pub centre: Vec2,
    pub forward: Vec2,
    pub half_along: f32,
    pub half_across: f32,
    pub slack: f32,
}

impl Footprint {
    #[must_use]
    pub fn new(
        centre: Vec2,
        facing: Facing,
        half_along: f32,
        half_across: f32,
        slack: f32,
    ) -> Self {
        let clean = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
        Self {
            centre,
            forward: facing.to_vector(),
            half_along: clean(half_along),
            half_across: clean(half_across),
            slack: if slack.is_finite() { slack } else { 0.0 },
        }
    }

    #[must_use]
    pub const fn at(self, centre: Vec2) -> Self {
        Self { centre, ..self }
    }

    #[must_use]
    pub fn enclosing_radius(&self) -> f32 {
        Vec2::new(self.half_along, self.half_across).length()
    }

    fn left(&self) -> Vec2 {
        Vec2::new(-self.forward.y, self.forward.x)
    }

    fn reach_along(&self, axis: Vec2) -> f32 {
        self.forward.dot(axis).abs() * self.half_along
            + self.left().dot(axis).abs() * self.half_across
    }

    #[must_use]
    pub fn penetration(&self, other: &Self) -> f32 {
        let between = other.centre.minus(self.centre);
        [self.forward, self.left(), other.forward, other.left()]
            .into_iter()
            .map(|axis| self.reach_along(axis) + other.reach_along(axis) - between.dot(axis).abs())
            .fold(f32::INFINITY, f32::min)
    }

    #[must_use]
    pub fn crowds(&self, other: &Self) -> bool {
        self.penetration(other) > self.slack.min(other.slack)
    }
}

#[derive(Debug, Clone)]
pub struct Claims {
    bucket: f32,
    reach: f32,
    gap: f32,
    footprints: Vec<Footprint>,
    buckets: BTreeMap<(i32, i32), Vec<usize>>,
}

impl Claims {
    #[must_use]
    pub fn new(bucket: f32) -> Self {
        Self {
            bucket: if bucket.is_finite() && bucket >= 1.0 {
                bucket
            } else {
                1.0
            },
            reach: 0.0,
            gap: 0.0,
            footprints: Vec::new(),
            buckets: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.footprints.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.footprints.is_empty()
    }

    #[allow(clippy::cast_possible_truncation)]
    fn key(&self, v: f32) -> i32 {
        let k = (v / self.bucket).floor();
        if k.is_finite() {
            k.clamp(-1.0e9, 1.0e9) as i32
        } else {
            0
        }
    }

    pub fn insert(&mut self, footprint: Footprint) {
        let index = self.footprints.len();
        let key = (self.key(footprint.centre.x), self.key(footprint.centre.y));
        self.buckets.entry(key).or_default().push(index);
        self.reach = self.reach.max(footprint.enclosing_radius());
        self.gap = self.gap.max(-footprint.slack);
        self.footprints.push(footprint);
    }

    #[must_use]
    pub fn crowded(&self, candidate: &Footprint) -> bool {
        self.excess(candidate) > 0.0
    }

    #[must_use]
    pub fn excess(&self, candidate: &Footprint) -> f32 {
        let own = candidate.enclosing_radius();
        let gap = self.gap.max(-candidate.slack).max(0.0);
        let range = core::f32::consts::SQRT_2 * (own + self.reach + gap);
        let (x0, y0) = (
            self.key(candidate.centre.x - range),
            self.key(candidate.centre.y - range),
        );
        let (x1, y1) = (
            self.key(candidate.centre.x + range),
            self.key(candidate.centre.y + range),
        );
        if x1.saturating_sub(x0) > MAX_BUCKET_SPAN || y1.saturating_sub(y0) > MAX_BUCKET_SPAN {
            return f32::INFINITY;
        }
        let mut worst = f32::NEG_INFINITY;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let Some(held) = self.buckets.get(&(x, y)) else {
                    continue;
                };
                for claim in held.iter().filter_map(|&i| self.footprints.get(i)) {
                    let allowed = claim.slack.min(candidate.slack);
                    let limit = own + claim.enclosing_radius() - allowed;
                    let apart = claim.centre.minus(candidate.centre).length_squared();
                    if limit <= 0.0 || apart >= 2.0 * limit * limit {
                        continue;
                    }
                    worst = worst.max(candidate.penetration(claim) - allowed);
                }
            }
        }
        worst
    }
}

struct Near<'a> {
    claims: Vec<&'a Footprint>,
    cells: Vec<Vec<usize>>,
    origin: Vec2,
    size: f32,
    side: usize,
    range: f32,
    last: Option<usize>,
}

impl<'a> Near<'a> {
    fn gather(claims: &'a Claims, desired: &Footprint, span: f32) -> Option<Self> {
        let gap = claims.gap.max(-desired.slack).max(0.0);
        let range = core::f32::consts::SQRT_2 * (desired.enclosing_radius() + claims.reach + gap);
        let reach = span + range;
        let (x0, y0) = (
            claims.key(desired.centre.x - reach),
            claims.key(desired.centre.y - reach),
        );
        let (x1, y1) = (
            claims.key(desired.centre.x + reach),
            claims.key(desired.centre.y + reach),
        );
        if x1.saturating_sub(x0) > MAX_BUCKET_SPAN || y1.saturating_sub(y0) > MAX_BUCKET_SPAN {
            return None;
        }
        let size = claims.bucket;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let side = ((2.0 * reach / size).ceil().max(0.0) as usize).saturating_add(1);
        let mut near = Self {
            claims: Vec::new(),
            cells: vec![Vec::new(); side.saturating_mul(side)],
            origin: desired.centre.minus(Vec2::new(reach, reach)),
            size,
            side,
            range,
            last: None,
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let Some(held) = claims.buckets.get(&(x, y)) else {
                    continue;
                };
                for claim in held.iter().filter_map(|&i| claims.footprints.get(i)) {
                    let at = claim.centre.minus(near.origin);
                    let cell = near
                        .column(at.y)
                        .checked_mul(near.side)
                        .and_then(|row| row.checked_add(near.column(at.x)));
                    if let Some(list) = cell.and_then(|cell| near.cells.get_mut(cell)) {
                        list.push(near.claims.len());
                        near.claims.push(claim);
                    }
                }
            }
        }
        Some(near)
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn column(&self, v: f32) -> usize {
        #[allow(clippy::cast_precision_loss)]
        let last = self.side.saturating_sub(1) as f32;
        let k = (v / self.size).floor();
        if k.is_finite() {
            k.clamp(0.0, last) as usize
        } else {
            0
        }
    }

    fn excess(&mut self, candidate: &Footprint) -> f32 {
        let press = |claim: &Footprint| {
            let allowed = claim.slack.min(candidate.slack);
            let limit = candidate.enclosing_radius() + claim.enclosing_radius() - allowed;
            let apart = claim.centre.minus(candidate.centre).length_squared();
            if limit <= 0.0 || apart >= 2.0 * limit * limit {
                return f32::NEG_INFINITY;
            }
            candidate.penetration(claim) - allowed
        };
        if let Some(claim) = self.last.and_then(|i| self.claims.get(i)) {
            let excess = press(claim);
            if excess > 0.0 {
                return excess;
            }
        }
        let at = candidate.centre.minus(self.origin);
        let (x0, x1) = (
            self.column(at.x - self.range),
            self.column(at.x + self.range),
        );
        let (y0, y1) = (
            self.column(at.y - self.range),
            self.column(at.y + self.range),
        );
        let mut found = None;
        'cells: for y in y0..=y1 {
            for x in x0..=x1 {
                let Some(list) = y
                    .checked_mul(self.side)
                    .and_then(|row| row.checked_add(x))
                    .and_then(|cell| self.cells.get(cell))
                else {
                    continue;
                };
                for &index in list {
                    let excess = self
                        .claims
                        .get(index)
                        .map_or(f32::NEG_INFINITY, |c| press(c));
                    if excess > 0.0 {
                        found = Some((index, excess));
                        break 'cells;
                    }
                }
            }
        }
        match found {
            Some((index, excess)) => {
                self.last = Some(index);
                excess
            }
            None => f32::NEG_INFINITY,
        }
    }
}

const REACH: i32 = 26;

fn ring_offsets() -> &'static [(i32, i32)] {
    static RINGS: OnceLock<Vec<(i32, i32)>> = OnceLock::new();
    RINGS.get_or_init(|| {
        let mut offsets = Vec::new();
        for y in -REACH..=REACH {
            for x in -REACH..=REACH {
                offsets.push((x, y));
            }
        }
        offsets.sort_by_key(|&(x, y)| {
            (
                x.saturating_mul(x).saturating_add(y.saturating_mul(y)),
                y,
                x,
            )
        });
        offsets.truncate(MAX_CANDIDATES);
        offsets
    })
}

pub fn nearest_free(
    claims: &Claims,
    desired: Footprint,
    step: f32,
    mut accept: impl FnMut(Vec2) -> bool,
) -> Option<Footprint> {
    let step = if step.is_finite() && step > 0.0 {
        step
    } else {
        1.0
    };
    let side = REACH.saturating_mul(2).saturating_add(1);
    let index = |x: i32, y: i32| -> Option<usize> {
        if x.abs() > REACH || y.abs() > REACH {
            return None;
        }
        let row = usize::try_from(y.saturating_add(REACH)).ok()?;
        let column = usize::try_from(x.saturating_add(REACH)).ok()?;
        row.checked_mul(usize::try_from(side).ok()?)?
            .checked_add(column)
    };
    let mut ruled_out = vec![false; usize::try_from(side.saturating_mul(side)).unwrap_or(0)];
    #[allow(clippy::cast_precision_loss)]
    let span = step * REACH as f32;
    let mut near = Near::gather(claims, &desired, span)?;
    for &(x, y) in ring_offsets() {
        if index(x, y)
            .and_then(|i| ruled_out.get(i))
            .copied()
            .unwrap_or(true)
        {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let offset = Vec2::new(x as f32 * step, y as f32 * step);
        let candidate = desired.at(desired.centre.plus(offset));
        let excess = near.excess(&candidate);
        if excess <= 0.0 {
            if accept(candidate.centre) {
                return Some(candidate);
            }
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let clear = (excess - 1.0e-3).min(step * 2.0 * REACH as f32);
        #[allow(clippy::cast_possible_truncation)]
        let cells = (clear / step).floor() as i32;
        for dy in cells.saturating_neg()..=cells {
            for dx in cells.saturating_neg()..=cells {
                #[allow(clippy::cast_precision_loss)]
                let distance = Vec2::new(dx as f32 * step, dy as f32 * step).length();
                if distance >= clear {
                    continue;
                }
                if let Some(slot) = index(x.saturating_add(dx), y.saturating_add(dy))
                    .and_then(|i| ruled_out.get_mut(i))
                {
                    *slot = true;
                }
            }
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    fn block(x: f32, y: f32) -> Footprint {
        Footprint::new(Vec2::new(x, y), Facing::EAST, 28.0, 48.0, 8.0)
    }

    #[test]
    fn separated_boxes_do_not_penetrate_and_stacked_ones_do() {
        let a = block(0.0, 0.0);
        assert!(a.penetration(&block(0.0, 200.0)) <= 0.0);
        assert!((a.penetration(&block(0.0, 0.0)) - 56.0).abs() < 1e-3);
        assert!(a.penetration(&block(0.0, 96.0)).abs() < 1e-3);
        assert!(!a.crowds(&block(0.0, 90.0)), "6 of overlap is within slack");
        assert!(a.crowds(&block(0.0, 80.0)), "16 is not");
    }

    #[test]
    fn a_turned_box_is_measured_on_its_own_axes() {
        let a = block(0.0, 0.0);
        let turned = Footprint::new(
            Vec2::new(90.0, 0.0),
            Facing::from_radians(core::f32::consts::FRAC_PI_2),
            28.0,
            48.0,
            8.0,
        );
        assert!(!a.crowds(&turned));
        assert!(a.crowds(&turned.at(Vec2::new(60.0, 0.0))));
    }

    #[test]
    fn a_negative_slack_is_a_gap_to_keep() {
        let spaced = |x: f32| Footprint::new(Vec2::new(x, 0.0), Facing::EAST, 28.0, 48.0, -5.0);
        assert!(spaced(0.0).crowds(&spaced(58.0)), "2 apart, 5 wanted");
        assert!(!spaced(0.0).crowds(&spaced(62.0)));
        assert!(block(0.0, 0.0).crowds(&spaced(58.0)), "the gap wins");
    }

    fn every_spot(claims: &Claims, desired: Footprint, step: f32) -> Option<Footprint> {
        ring_offsets().iter().find_map(|&(x, y)| {
            let candidate = desired.at(desired
                .centre
                .plus(Vec2::new(x as f32 * step, y as f32 * step)));
            (!claims.crowded(&candidate)).then_some(candidate)
        })
    }

    #[test]
    fn skipping_finds_the_same_spot_as_testing_every_spot() {
        let mut claims = Claims::new(128.0);
        for i in 0..144u16 {
            if i % 7 == 3 {
                continue;
            }
            let (col, row) = (f32::from(i % 12), f32::from(i / 12));
            claims.insert(Footprint::new(
                Vec2::new(col * 101.0 - 600.0, row * 63.0 - 380.0),
                Facing::from_radians(f32::from(i % 5) * 0.1),
                28.0,
                48.0,
                -5.0,
            ));
        }
        for (k, (x, y)) in [(0.0, 0.0), (37.0, -12.0), (-250.0, 90.0), (600.0, 400.0)]
            .into_iter()
            .enumerate()
        {
            #[allow(clippy::cast_precision_loss)]
            let facing = Facing::from_radians(k as f32 * 0.4);
            let desired = Footprint::new(Vec2::new(x, y), facing, 28.0, 48.0, -5.0);
            for step in [10.0f32, 14.0, 28.0] {
                let fast = nearest_free(&claims, desired, step, |_| true).map(|f| f.centre);
                let slow = every_spot(&claims, desired, step).map(|f| f.centre);
                assert_eq!(fast, slow, "desired ({x}, {y}) step {step}");
            }
        }
    }

    #[test]
    #[ignore = "timing, run by hand in release"]
    fn placing_into_an_army_costs() {
        let block =
            |x: f32, y: f32| Footprint::new(Vec2::new(x, y), Facing::EAST, 28.0, 48.0, -5.0);
        for (side, placing) in [(10u16, 100u16), (20, 400)] {
            let mut claims = Claims::new(128.0);
            for i in 0..side * side {
                claims.insert(block(
                    f32::from(i % side) * 100.0 - f32::from(side) * 50.0,
                    f32::from(i / side) * 110.0 - f32::from(side) * 55.0,
                ));
            }
            let started = std::time::Instant::now();
            let (mut far, mut unplaced) = (0.0f32, 0u32);
            for _ in 0..placing {
                let found = [14.0f32, 28.0, 56.0]
                    .into_iter()
                    .find_map(|step| nearest_free(&claims, block(0.0, 0.0), step, |_| true));
                match found {
                    Some(spot) => {
                        far = far.max(spot.centre.length());
                        claims.insert(spot);
                    }
                    None => unplaced += 1,
                }
            }
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            println!(
                "placing into an army of {}: {placing} in {ms:.2} ms ({:.3} each), \
                 farthest {far:.0}, {unplaced} found no spot",
                side * side,
                ms / f64::from(placing)
            );
        }
    }

    #[test]
    fn a_free_desired_spot_is_kept() {
        let claims = Claims::new(64.0);
        let spot = nearest_free(&claims, block(5.0, 5.0), 10.0, |_| true).unwrap();
        assert_eq!(spot.centre, Vec2::new(5.0, 5.0));
    }

    #[test]
    fn a_held_spot_moves_the_next_mover_beside_it_not_onto_it() {
        let mut claims = Claims::new(64.0);
        claims.insert(block(0.0, 0.0));
        let spot = nearest_free(&claims, block(0.0, 0.0), 10.0, |_| true).unwrap();
        assert!(!spot.crowds(&block(0.0, 0.0)));
        assert!(
            spot.centre.length() <= 60.0 + 1e-3,
            "nearest clear spot, not a far one: {:?}",
            spot.centre
        );
    }

    #[test]
    fn many_movers_to_one_point_pack_without_crowding() {
        let mut claims = Claims::new(64.0);
        let mut placed = Vec::new();
        for _ in 0..9 {
            let spot = nearest_free(&claims, block(0.0, 0.0), 10.0, |_| true).unwrap();
            claims.insert(spot);
            placed.push(spot);
        }
        for (i, a) in placed.iter().enumerate() {
            for b in placed.iter().skip(i + 1) {
                assert!(!a.crowds(b), "{a:?} crowds {b:?}");
            }
            assert!(a.centre.length() < 200.0, "clustered: {:?}", a.centre);
        }
    }

    #[test]
    fn a_refused_spot_is_skipped_and_nothing_is_none() {
        let claims = Claims::new(64.0);
        let spot = nearest_free(&claims, block(0.0, 0.0), 10.0, |p| p.x >= 30.0).unwrap();
        assert_eq!(spot.centre, Vec2::new(30.0, 0.0));
        assert!(nearest_free(&claims, block(0.0, 0.0), 10.0, |_| false).is_none());
    }
}
