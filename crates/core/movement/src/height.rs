use crate::geom::{CellGeometry, Vec2};
use crate::grid::GridExtent;
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct HeightField {
    extent: GridExtent,
    geometry: CellGeometry,
    samples: Vec<f32>,
}

impl HeightField {
    pub fn new(extent: GridExtent, geometry: CellGeometry, samples: Vec<f32>) -> Result<Self> {
        let expected = extent.cell_count();
        let given = u32::try_from(samples.len()).unwrap_or(u32::MAX);
        if given != expected {
            return Err(Error::HeightFieldSize { given, expected });
        }
        Ok(Self {
            extent,
            geometry,
            samples,
        })
    }

    #[must_use]
    pub const fn extent(&self) -> GridExtent {
        self.extent
    }

    #[must_use]
    pub const fn geometry(&self) -> CellGeometry {
        self.geometry
    }

    #[must_use]
    pub fn height_at(&self, x: f32, y: f32) -> Option<f32> {
        let size = self.geometry.cell_size();
        let origin = self.geometry.origin();
        let gx = (x - origin.x) / size;
        let gy = (y - origin.y) / size;
        if !gx.is_finite() || !gy.is_finite() || gx < 0.0 || gy < 0.0 {
            return None;
        }
        let (fx, fy) = (gx - gx.floor(), gy - gy.floor());
        let x0 = node_index(gx, self.extent.width)?;
        let y0 = node_index(gy, self.extent.height)?;
        let last_x = self.extent.width.saturating_sub(1);
        let last_y = self.extent.height.saturating_sub(1);
        let sample = |dx: u16, dy: u16| {
            self.sample(
                x0.saturating_add(dx).min(last_x),
                y0.saturating_add(dy).min(last_y),
            )
        };
        let (h00, h10, h01, h11) = (sample(0, 0)?, sample(1, 0)?, sample(0, 1)?, sample(1, 1)?);
        let top = h00 + (h10 - h00) * fx;
        let bottom = h01 + (h11 - h01) * fx;
        Some(top + (bottom - top) * fy)
    }

    #[must_use]
    pub fn triangle_height_at(&self, x: f32, y: f32) -> Option<f32> {
        let size = self.geometry.cell_size();
        let origin = self.geometry.origin();
        let gx = (x - origin.x) / size;
        let gy = (y - origin.y) / size;
        if !gx.is_finite() || !gy.is_finite() || gx < 0.0 || gy < 0.0 {
            return None;
        }
        let (fx, fy) = (gx - gx.floor(), gy - gy.floor());
        let x0 = node_index(gx, self.extent.width)?;
        let y0 = node_index(gy, self.extent.height)?;
        let last_x = self.extent.width.saturating_sub(1);
        let last_y = self.extent.height.saturating_sub(1);
        let sample = |dx: u16, dy: u16| {
            self.sample(
                x0.saturating_add(dx).min(last_x),
                y0.saturating_add(dy).min(last_y),
            )
        };
        let (p0, p2) = (sample(0, 0)?, sample(1, 1)?);
        if fy > fx {
            let p3 = sample(0, 1)?;
            Some(p3 + (1.0 - fy) * (p0 - p3) + fx * (p2 - p3))
        } else {
            let p1 = sample(1, 0)?;
            Some(p1 + fy * (p2 - p1) + (1.0 - fx) * (p0 - p1))
        }
    }

    #[must_use]
    pub fn height_at_point(&self, point: Vec2) -> Option<f32> {
        self.height_at(point.x, point.y)
    }

    #[must_use]
    pub fn sample(&self, x: u16, y: u16) -> Option<f32> {
        if x >= self.extent.width || y >= self.extent.height {
            return None;
        }
        let index = usize::from(y).checked_mul(usize::from(self.extent.width))?;
        self.samples
            .get(index.checked_add(usize::from(x))?)
            .copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeightRule {
    Surface { above: f32, damping: Option<f32> },
    Fixed { height: f32, damping: Option<f32> },
    Unchanged,
}

impl HeightRule {
    pub const ON_SURFACE: Self = Self::Surface {
        above: 0.0,
        damping: None,
    };

    #[must_use]
    pub const fn level(self) -> f32 {
        match self {
            Self::Surface { above, .. } => above,
            Self::Fixed { height, .. } => height,
            Self::Unchanged => 0.0,
        }
    }

    #[must_use]
    pub const fn damping(self) -> Option<f32> {
        match self {
            Self::Surface { damping, .. } | Self::Fixed { damping, .. } => damping,
            Self::Unchanged => None,
        }
    }

    #[must_use]
    pub const fn undamped(self) -> Self {
        match self {
            Self::Surface { above, .. } => Self::Surface {
                above,
                damping: None,
            },
            Self::Fixed { height, .. } => Self::Fixed {
                height,
                damping: None,
            },
            Self::Unchanged => Self::Unchanged,
        }
    }

    #[must_use]
    pub const fn at_level(self, level: f32) -> Self {
        match self {
            Self::Surface { damping, .. } => Self::Surface {
                above: level,
                damping,
            },
            Self::Fixed { damping, .. } => Self::Fixed {
                height: level,
                damping,
            },
            Self::Unchanged => Self::Unchanged,
        }
    }

    #[must_use]
    pub fn height_at(self, field: &HeightField, point: Vec2) -> Option<f32> {
        match self {
            Self::Surface { above, .. } => {
                field.height_at_point(point).map(|ground| ground + above)
            }
            Self::Fixed { height, .. } => Some(height),
            Self::Unchanged => None,
        }
    }
}

#[must_use]
pub fn step_damped(current: f32, target: f32, damping: f32, logic_hz: f32, ticks: u16) -> f32 {
    if logic_hz <= 0.0 {
        return target;
    }
    let share = damping / (2.0 * logic_hz);
    let share = if ticks <= 1 {
        share
    } else if share < 1.0 {
        1.0 - mint_math::f32::powf(1.0 - share, 1.0 / f32::from(ticks))
    } else {
        1.0
    };
    current + (target - current) * share
}

#[must_use]
pub fn step_level(current: f32, goal: f32, max_step: f32) -> f32 {
    let gap = goal - current;
    if !(max_step.is_finite() && max_step >= 0.0) || gap.abs() <= max_step {
        goal
    } else if gap > 0.0 {
        current + max_step
    } else {
        current - max_step
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttackPhase {
    #[default]
    Cruise,
    Descending,
    Climbing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackHeightInput {
    pub preferred: f32,
    pub attack: f32,
    pub circling_radius: f32,
    pub distance: f32,
    pub above: f32,
    pub armed: bool,
    pub attacking: bool,
    pub fired: bool,
}

#[must_use]
pub fn attack_height(phase: AttackPhase, input: AttackHeightInput) -> (AttackPhase, f32) {
    let AttackHeightInput {
        preferred,
        attack,
        circling_radius,
        distance,
        above,
        armed,
        attacking,
        fired,
    } = input;
    if !armed || circling_radius <= 0.0 {
        return (AttackPhase::Cruise, preferred);
    }
    let blend = |share: f32| attack + (preferred - attack) * share.min(1.0);
    match phase {
        AttackPhase::Cruise => {
            let next = if distance < circling_radius && attacking {
                AttackPhase::Descending
            } else {
                AttackPhase::Cruise
            };
            (next, preferred)
        }
        AttackPhase::Descending => {
            let next = if fired || !attacking {
                AttackPhase::Climbing
            } else {
                AttackPhase::Descending
            };
            (next, blend(distance / circling_radius))
        }
        AttackPhase::Climbing => {
            let next = if above >= preferred {
                AttackPhase::Cruise
            } else {
                AttackPhase::Climbing
            };
            (next, blend(2.0 * distance / circling_radius))
        }
    }
}

fn node_index(value: f32, limit: u16) -> Option<u16> {
    let floored = value.floor();
    if floored >= f32::from(u16::MAX) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = floored as u16;
    (index < limit).then_some(index)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ridge() -> HeightField {
        HeightField::new(
            GridExtent::new(3, 3).unwrap(),
            CellGeometry::new(10.0, Vec2::ZERO).unwrap(),
            vec![0.0, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 0.0],
        )
        .unwrap()
    }

    #[test]
    fn a_sample_between_four_nodes_is_their_bilinear_blend() {
        let field = ridge();
        assert_eq!(field.height_at(10.0, 10.0), Some(10.0));
        assert_eq!(field.height_at(5.0, 10.0), Some(5.0));
        assert_eq!(field.height_at(10.0, 5.0), Some(5.0));
        assert_eq!(field.height_at(5.0, 5.0), Some(2.5));
    }

    #[test]
    fn a_point_off_the_field_has_no_height() {
        let field = ridge();
        assert_eq!(field.height_at(-0.5, 0.0), None);
        assert_eq!(field.height_at(0.0, 30.0), None);
        assert_eq!(field.height_at(f32::NAN, 0.0), None);
    }

    fn input(distance: f32, armed: bool, attacking: bool, fired: bool) -> AttackHeightInput {
        AttackHeightInput {
            preferred: 100.0,
            attack: 10.0,
            circling_radius: 200.0,
            distance,
            above: 100.0,
            armed,
            attacking,
            fired,
        }
    }

    #[test]
    fn an_attacking_flier_blends_down_over_its_circling_radius_and_climbs_back_twice_as_fast() {
        let (phase, level) = attack_height(AttackPhase::Cruise, input(100.0, true, true, false));
        assert_eq!((phase, level), (AttackPhase::Descending, 100.0));
        let (phase, level) = attack_height(phase, input(100.0, true, true, false));
        assert_eq!((phase, level), (AttackPhase::Descending, 55.0));
        let (phase, level) = attack_height(phase, input(100.0, true, true, true));
        assert_eq!((phase, level), (AttackPhase::Climbing, 55.0));
        let (_, level) = attack_height(phase, input(50.0, true, true, true));
        assert_eq!(level, 55.0);
        let mut back = input(100.0, true, true, true);
        back.above = 100.0;
        assert_eq!(attack_height(phase, back).0, AttackPhase::Cruise);
    }

    #[test]
    fn a_flier_without_a_weapon_or_a_circling_radius_holds_its_preferred_height() {
        let unarmed = input(100.0, false, true, false);
        assert_eq!(
            attack_height(AttackPhase::Descending, unarmed),
            (AttackPhase::Cruise, 100.0)
        );
        let mut straight = input(100.0, true, true, false);
        straight.circling_radius = 0.0;
        assert_eq!(
            attack_height(AttackPhase::Cruise, straight),
            (AttackPhase::Cruise, 100.0)
        );
    }

    #[test]
    fn a_level_steps_toward_its_goal_and_lands_on_it() {
        assert_eq!(step_level(121.0, 5.0, 40.0), 81.0);
        assert_eq!(step_level(5.0, 121.0, 40.0), 45.0);
        assert_eq!(step_level(30.0, 5.0, 40.0), 5.0);
        assert_eq!(step_level(121.0, 5.0, f32::INFINITY), 5.0);
    }

    #[test]
    fn a_damped_step_closes_a_share_of_the_gap_each_frame_rather_than_landing_on_it() {
        let step = step_damped(0.0, 100.0, 1.0, 5.0, 1);
        assert_eq!(step, 10.0);
        let step = step_damped(step, 100.0, 1.0, 5.0, 1);
        assert_eq!(step, 19.0);
    }

    #[test]
    fn a_damped_step_split_over_ticks_rises_every_tick_and_lands_where_one_frame_does() {
        let mut z = 0.0;
        let mut rises = Vec::new();
        for _ in 0..6 {
            let next = step_damped(z, 100.0, 1.0, 5.0, 6);
            rises.push(next - z);
            z = next;
        }
        assert!(rises.iter().all(|rise| *rise > 1.0), "{rises:?}");
        assert!((z - 10.0).abs() < 1e-3, "{z}");
    }

    #[test]
    fn a_sample_count_that_disagrees_with_the_extent_is_refused() {
        let err = HeightField::new(
            GridExtent::new(3, 3).unwrap(),
            CellGeometry::new(10.0, Vec2::ZERO).unwrap(),
            vec![0.0; 8],
        )
        .unwrap_err();
        assert!(matches!(err, Error::HeightFieldSize { .. }));
    }
}
