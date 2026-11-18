use crate::geom::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MoverId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Agent {
    pub id: MoverId,
    pub position: Vec2,
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Neighbour {
    pub id: MoverId,
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
}

pub trait NeighbourQuery {
    fn neighbours(&self, centre: Vec2, radius: f32, out: &mut Vec<Neighbour>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoNeighbours;

impl NeighbourQuery for NoNeighbours {
    fn neighbours(&self, _centre: Vec2, _radius: f32, _out: &mut Vec<Neighbour>) {}
}

pub trait Avoidance {
    fn query_radius(&self, agent: &Agent) -> f32;

    fn adjust(
        &self,
        agent: &Agent,
        desired: Vec2,
        query: &dyn NeighbourQuery,
        scratch: &mut Vec<Neighbour>,
    ) -> Vec2;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoAvoidance;

impl Avoidance for NoAvoidance {
    fn query_radius(&self, _agent: &Agent) -> f32 {
        0.0
    }

    fn adjust(
        &self,
        _agent: &Agent,
        desired: Vec2,
        _query: &dyn NeighbourQuery,
        _scratch: &mut Vec<Neighbour>,
    ) -> Vec2 {
        desired
    }
}

const NEIGHBOUR_QUERY_MARGIN: f32 = 80.0;

const SEPARATION_GAIN: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeparationAvoidance {
    margin: f32,
}

impl Default for SeparationAvoidance {
    fn default() -> Self {
        Self::new(Self::HORDE_MARGIN)
    }
}

impl SeparationAvoidance {
    pub const HORDE_MARGIN: f32 = NEIGHBOUR_QUERY_MARGIN;

    pub const MEMBER_MARGIN: f32 = 20.0;

    #[must_use]
    pub const fn new(margin: f32) -> Self {
        Self { margin }
    }
}

impl Avoidance for SeparationAvoidance {
    fn query_radius(&self, agent: &Agent) -> f32 {
        agent.radius.max(0.0) + self.margin.max(0.0)
    }

    fn adjust(
        &self,
        agent: &Agent,
        desired: Vec2,
        query: &dyn NeighbourQuery,
        scratch: &mut Vec<Neighbour>,
    ) -> Vec2 {
        scratch.clear();
        query.neighbours(agent.position, self.query_radius(agent), scratch);
        let mut push = Vec2::ZERO;
        for neighbour in scratch.iter() {
            if neighbour.id == agent.id {
                continue;
            }
            let combined = agent.radius.max(0.0) + neighbour.radius.max(0.0);
            let delta = agent.position.minus(neighbour.position);
            let distance = delta.length();
            if distance.partial_cmp(&combined) != Some(core::cmp::Ordering::Less) {
                continue;
            }
            let overlap = combined - distance;
            let direction = delta.normalised().unwrap_or(Vec2::new(1.0, 0.0));
            push = push.plus(direction.scale(overlap * SEPARATION_GAIN));
        }
        desired.plus(push)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AvoidanceMode {
    #[default]
    Off,
    Separation,
}

impl AvoidanceMode {
    pub const ALL: [Self; 2] = [Self::Off, Self::Separation];

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Separation,
            Self::Separation => Self::Off,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "avoidance=off",
            Self::Separation => "avoidance=separation",
        }
    }

    #[must_use]
    pub fn build(self) -> Box<dyn Avoidance> {
        match self {
            Self::Off => Box::new(NoAvoidance),
            Self::Separation => Box::new(SeparationAvoidance::default()),
        }
    }
}

impl core::fmt::Display for AvoidanceMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stagger {
    period: u32,
}

impl Stagger {
    pub const EVERY_TICK: Self = Self { period: 1 };

    #[must_use]
    pub const fn every(period: u32) -> Self {
        Self {
            period: if period == 0 { 1 } else { period },
        }
    }

    #[must_use]
    pub const fn period(self) -> u32 {
        self.period
    }

    #[must_use]
    pub fn is_due(self, id: MoverId, frame: u64) -> bool {
        Some(self.slot(id)) == frame.checked_rem(u64::from(self.period))
    }

    #[must_use]
    pub fn frames_until_due(self, id: MoverId, frame: u64) -> u32 {
        let period = u64::from(self.period);
        let now = frame.checked_rem(period).unwrap_or(0);
        let ahead = self
            .slot(id)
            .wrapping_add(period)
            .wrapping_sub(now)
            .checked_rem(period)
            .unwrap_or(0);
        u32::try_from(ahead).map_or(self.period, |ahead| ahead.saturating_add(1))
    }

    fn slot(self, id: MoverId) -> u64 {
        const SCATTER: u32 = 0x9E37_79B9;
        let scattered = u64::from(id.0.wrapping_mul(SCATTER));
        scattered
            .wrapping_mul(u64::from(self.period))
            .checked_shr(32)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staggering_covers_every_mover_exactly_once_per_period() {
        let stagger = Stagger::every(4);
        for id in 0..40u32 {
            let due: Vec<u64> = (100..104)
                .filter(|f| stagger.is_due(MoverId(id), *f))
                .collect();
            assert_eq!(due.len(), 1, "mover {id} was due on {due:?} of four frames");
        }
    }

    #[test]
    fn frames_until_due_counts_down_to_the_due_frame() {
        let stagger = Stagger::every(6);
        for id in 0..40u32 {
            let id = MoverId(id);
            for frame in 100..112u64 {
                let left = stagger.frames_until_due(id, frame);
                assert!((1..=6).contains(&left));
                let due = frame + u64::from(left) - 1;
                assert!(stagger.is_due(id, due), "{id:?} not due on {due}");
                assert!(
                    (frame..due).all(|f| !stagger.is_due(id, f)),
                    "{id:?} due before {due}"
                );
            }
        }
    }

    #[test]
    fn ids_stepping_by_the_period_are_spread_over_its_frames() {
        let stagger = Stagger::every(6);
        for frame in 0..6u64 {
            let due = (0..600u32)
                .filter(|k| stagger.is_due(MoverId(k * 6 + 2), frame))
                .count();
            assert!(
                (80..=120).contains(&due),
                "{due} of 600 movers due on frame {frame}"
            );
        }
    }

    #[test]
    fn null_avoidance_returns_the_desired_velocity_untouched() {
        let agent = Agent {
            id: MoverId(7),
            position: Vec2::new(3.0, 4.0),
            radius: 5.0,
        };
        let desired = Vec2::new(-2.0, 11.0);
        let mut scratch = Vec::new();
        let adjusted = NoAvoidance.adjust(&agent, desired, &NoNeighbours, &mut scratch);
        assert_eq!(adjusted, desired);
    }

    struct FixedNeighbours(Vec<Neighbour>);

    impl NeighbourQuery for FixedNeighbours {
        fn neighbours(&self, _centre: Vec2, _radius: f32, out: &mut Vec<Neighbour>) {
            out.extend(self.0.iter().copied());
        }
    }

    fn neighbour(id: u32, position: Vec2, radius: f32) -> Neighbour {
        Neighbour {
            id: MoverId(id),
            position,
            velocity: Vec2::ZERO,
            radius,
        }
    }

    #[test]
    fn an_overlapping_neighbour_pushes_the_agent_directly_away() {
        let agent = Agent {
            id: MoverId(1),
            position: Vec2::new(0.0, 0.0),
            radius: 8.0,
        };
        let neighbours = FixedNeighbours(vec![neighbour(2, Vec2::new(10.0, 0.0), 8.0)]);
        let mut scratch = Vec::new();
        let adjusted =
            SeparationAvoidance::default().adjust(&agent, Vec2::ZERO, &neighbours, &mut scratch);
        assert!(
            adjusted.x < 0.0,
            "must push away from the neighbour: {adjusted:?}"
        );
        assert!(
            adjusted.y.abs() < 1e-4,
            "must push straight along the line between them: {adjusted:?}"
        );
        assert!((adjusted.x + 12.0).abs() < 1e-3, "{adjusted:?}");
    }

    #[test]
    fn a_non_overlapping_neighbour_changes_nothing() {
        let agent = Agent {
            id: MoverId(1),
            position: Vec2::new(0.0, 0.0),
            radius: 5.0,
        };
        let neighbours = FixedNeighbours(vec![neighbour(2, Vec2::new(50.0, 0.0), 5.0)]);
        let mut scratch = Vec::new();
        let desired = Vec2::new(3.0, -1.0);
        let adjusted =
            SeparationAvoidance::default().adjust(&agent, desired, &neighbours, &mut scratch);
        assert_eq!(adjusted, desired);
    }

    #[test]
    fn the_agents_own_id_in_the_neighbour_list_is_ignored() {
        let agent = Agent {
            id: MoverId(1),
            position: Vec2::new(5.0, 5.0),
            radius: 8.0,
        };
        let neighbours = FixedNeighbours(vec![neighbour(1, agent.position, agent.radius)]);
        let mut scratch = Vec::new();
        let adjusted = SeparationAvoidance::default().adjust(
            &agent,
            Vec2::new(1.0, 2.0),
            &neighbours,
            &mut scratch,
        );
        assert_eq!(adjusted, Vec2::new(1.0, 2.0));
    }

    #[test]
    fn summing_several_overlapping_neighbours_does_not_misbehave() {
        let agent = Agent {
            id: MoverId(1),
            position: Vec2::new(0.0, 0.0),
            radius: 8.0,
        };
        let a = neighbour(2, Vec2::new(10.0, 0.0), 8.0);
        let b = neighbour(3, Vec2::new(0.0, 9.0), 8.0);
        let c = neighbour(4, Vec2::new(-11.0, 0.0), 8.0);
        let ascending = FixedNeighbours(vec![a, b, c]);
        let mut scratch = Vec::new();
        let first =
            SeparationAvoidance::default().adjust(&agent, Vec2::ZERO, &ascending, &mut scratch);
        assert!(first.x.is_finite() && first.y.is_finite(), "{first:?}");
        let second =
            SeparationAvoidance::default().adjust(&agent, Vec2::ZERO, &ascending, &mut scratch);
        assert_eq!(first, second, "the same query twice must agree bit for bit");
    }
}
