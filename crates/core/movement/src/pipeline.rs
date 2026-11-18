use crate::avoidance::{Agent, Avoidance, AvoidanceMode, Neighbour, NeighbourQuery};
use crate::following::{
    Follower, Motion, MotionLimits, MotionPhase, MotionStatus, MoveContext, MoverState, Route,
    SlidingFollower, SteeringFollower,
};
use crate::geom::CellGeometry;
use crate::grid::Passability;
use crate::search::{
    AStar, CellPath, NodeBudget, Octile, PathRequest, Search, SearchOutcome, SearchResult,
    StraightLineSearch,
};
use crate::smoothing::{Smoothing, SmoothingMode};
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    StraightLine,
    #[default]
    AStar,
}

impl SearchMode {
    pub const ALL: [Self; 2] = [Self::StraightLine, Self::AStar];

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::StraightLine => Self::AStar,
            Self::AStar => Self::StraightLine,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::StraightLine => "search=straight-line",
            Self::AStar => "search=a*",
        }
    }

    #[must_use]
    pub fn build(self) -> Box<dyn Search> {
        match self {
            Self::StraightLine => Box::new(StraightLineSearch),
            Self::AStar => Box::new(AStar::octile()),
        }
    }
}

impl core::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PipelineConfig {
    pub search: SearchMode,
    pub smoothing: SmoothingMode,
    pub avoidance: AvoidanceMode,
}

impl PipelineConfig {
    #[must_use]
    pub const fn straight_line() -> Self {
        Self {
            search: SearchMode::StraightLine,
            smoothing: SmoothingMode::Off,
            avoidance: AvoidanceMode::Off,
        }
    }
}

impl core::fmt::Display for PipelineConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} {} {}", self.search, self.smoothing, self.avoidance)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub cells: CellPath,
    pub route: Route,
    pub expanded: u32,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PlanOutcome {
    Ready(Plan),
    Unreachable,
    BudgetExhausted,
    Closest(Plan),
}

pub struct Pipeline {
    config: PipelineConfig,
    search: Box<dyn Search>,
    first_visit: AStar<Octile>,
    smoothing: Box<dyn Smoothing>,
    follower: Box<dyn Follower>,
    avoidance: Box<dyn Avoidance>,
}

fn straight_result(grid: &dyn Passability, request: &PathRequest) -> Option<SearchResult> {
    if request.final_goal
        || request.start == request.goal
        || !crate::smoothing::straight_is_passable(grid, request.mover, request.start, request.goal)
    {
        return None;
    }
    Some(SearchResult {
        outcome: SearchOutcome::Found(CellPath {
            cells: vec![request.start, request.goal],
            cost: 0,
            hops: Vec::new(),
        }),
        expanded: 0,
    })
}

impl core::fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Pipeline")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Pipeline {
    #[must_use]
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            config,
            search: config.search.build(),
            first_visit: AStar::octile(),
            smoothing: config.smoothing.build(),
            follower: Box::new(SteeringFollower),
            avoidance: config.avoidance.build(),
        }
    }

    #[must_use]
    pub const fn config(&self) -> PipelineConfig {
        self.config
    }

    pub fn set_search(&mut self, mode: SearchMode) {
        self.config.search = mode;
        self.search = mode.build();
    }

    pub fn set_smoothing(&mut self, mode: SmoothingMode) {
        self.config.smoothing = mode;
        self.smoothing = mode.build();
    }

    pub fn set_avoidance(&mut self, mode: AvoidanceMode) {
        self.config.avoidance = mode;
        self.avoidance = mode.build();
    }

    pub fn use_sliding_follower(&mut self) {
        self.follower = Box::new(SlidingFollower);
    }

    pub fn plan(
        &mut self,
        grid: &dyn Passability,
        geometry: CellGeometry,
        request: &PathRequest,
        budget: &mut NodeBudget,
    ) -> Result<PlanOutcome> {
        let result = match straight_result(grid, request) {
            Some(result) => result,
            None => self.search.find(grid, request, budget)?,
        };
        self.finish(grid, geometry, request, result)
    }

    pub fn plan_first_visit(
        &mut self,
        grid: &dyn Passability,
        geometry: CellGeometry,
        request: &PathRequest,
        budget: &mut NodeBudget,
        refuse: &dyn Fn(crate::Cell) -> bool,
    ) -> Result<PlanOutcome> {
        let result = self
            .first_visit
            .find_first_visit(grid, request, budget, refuse)?;
        self.finish(grid, geometry, request, result)
    }

    pub fn plan_closest(
        &mut self,
        grid: &dyn Passability,
        geometry: CellGeometry,
        request: &PathRequest,
        budget: &mut NodeBudget,
    ) -> Result<PlanOutcome> {
        let result = match straight_result(grid, request) {
            Some(result) => result,
            None => self.search.find_closest(grid, request, budget)?,
        };
        self.finish(grid, geometry, request, result)
    }

    pub fn plan_closest_accepting(
        &mut self,
        grid: &dyn Passability,
        geometry: CellGeometry,
        request: &PathRequest,
        budget: &mut NodeBudget,
        accept: &dyn Fn(crate::Cell) -> bool,
        cost_weight: f32,
    ) -> Result<PlanOutcome> {
        let result = match straight_result(grid, request) {
            Some(result) => result,
            None => {
                self.search
                    .find_closest_accepting(grid, request, budget, accept, cost_weight)?
            }
        };
        self.finish(grid, geometry, request, result)
    }

    fn finish(
        &mut self,
        grid: &dyn Passability,
        geometry: CellGeometry,
        request: &PathRequest,
        result: SearchResult,
    ) -> Result<PlanOutcome> {
        let (path, exact) = match result.outcome {
            SearchOutcome::Found(path) => (path, true),
            SearchOutcome::Closest(path) => (path, false),
            SearchOutcome::Unreachable => return Ok(PlanOutcome::Unreachable),
            SearchOutcome::BudgetExhausted => return Ok(PlanOutcome::BudgetExhausted),
        };
        let heading = request
            .approach
            .map(|facing| crate::smoothing::final_heading(&path.cells, (facing.x, facing.y)));
        let (cells, mut route) = if path.hops.is_empty() {
            let cells = match heading {
                Some(heading) => {
                    self.smoothing
                        .smooth_backward(grid, request.mover, &path, heading, true)
                }
                None => self.smoothing.smooth(grid, request.mover, &path),
            };
            let route = self.route_of(grid, geometry, &cells);
            (cells, route)
        } else {
            self.route_through_portals(grid, geometry, request.mover, &path, heading)
        };
        if let Some(origin) = request.origin {
            match route.points.len() {
                0 => {}
                1 => {
                    route.points.insert(0, origin);
                    route.portals.insert(0, None);
                }
                _ => {
                    if let Some(first) = route.points.first_mut() {
                        *first = origin;
                    }
                }
            }
        }
        let clear = |point: crate::geom::Vec2| {
            geometry
                .cell_at(point, 0, grid.extent())
                .is_some_and(|cell| grid.is_passable(cell, request.mover))
        };
        crate::following::round_corners(&mut route, request.corner_radius, clear);
        if let (Some(departure), Some(_)) = (request.departure, request.origin) {
            let line_clear = |from: crate::geom::Vec2, to: crate::geom::Vec2| match (
                geometry.cell_at(from, 0, grid.extent()),
                geometry.cell_at(to, 0, grid.extent()),
            ) {
                (Some(a), Some(b)) => crate::smoothing::line_is_passable(grid, request.mover, a, b),
                _ => false,
            };
            crate::following::depart(&mut route, departure, clear, line_clear);
        }
        let plan = Plan {
            cells,
            route,
            expanded: result.expanded,
        };
        Ok(if exact {
            PlanOutcome::Ready(plan)
        } else {
            PlanOutcome::Closest(plan)
        })
    }

    fn route_of(&self, grid: &dyn Passability, geometry: CellGeometry, cells: &CellPath) -> Route {
        let mut route = Route::from_path(cells, geometry);
        for (tag, cell) in route.portals.iter_mut().zip(cells.cells.iter()) {
            *tag = grid.portal_at(*cell);
        }
        route
    }

    fn route_through_portals(
        &self,
        grid: &dyn Passability,
        geometry: CellGeometry,
        mover: crate::grid::MoverClass,
        path: &CellPath,
        heading: Option<(f32, f32)>,
    ) -> (CellPath, Route) {
        let path_end = path.cells.last().copied();
        let smooth = |stretch: Vec<crate::grid::Cell>,
                      last_stretch: bool,
                      edge: crate::smoothing::PortalEdge| {
            let stretch = CellPath {
                cells: stretch,
                cost: 0,
                hops: Vec::new(),
            };
            match (heading, path_end) {
                (Some(heading), _) => {
                    self.smoothing
                        .smooth_backward(grid, mover, &stretch, heading, last_stretch)
                }
                (None, Some(end)) => self
                    .smoothing
                    .smooth_by_portal(grid, mover, &stretch, end, edge),
                (None, None) => self.smoothing.smooth(grid, mover, &stretch),
            }
        };
        let mut cells = CellPath {
            cells: Vec::new(),
            cost: path.cost,
            hops: Vec::new(),
        };
        let mut route = Route::default();
        let mut start = 0usize;
        let mut after: Option<crate::grid::Portal> = None;
        for hop in &path.hops {
            let stretch: Vec<_> = path
                .cells
                .get(start..hop.to)
                .map(<[_]>::to_vec)
                .unwrap_or_default();
            let entry = hop
                .to
                .checked_sub(1)
                .and_then(|i| path.cells.get(i))
                .copied();
            let exit = path.cells.get(hop.to).copied();
            let link = entry.zip(exit).and_then(|(entry, exit)| {
                grid.portal_links(entry)
                    .iter()
                    .find(|link| link.slot == hop.slot && link.exit == exit)
            });
            if !stretch.is_empty() {
                let edge = crate::smoothing::PortalEdge {
                    into_portal: link.is_some(),
                    from_portal: after.is_some(),
                };
                let mut smoothed = smooth(stretch, false, edge);
                if edge.into_portal
                    && smoothed.cells.len() > 1
                    && smoothed
                        .cells
                        .last()
                        .is_some_and(|cell| grid.portal_at(*cell).is_none())
                {
                    smoothed.cells.pop();
                }
                let piece = self.route_of(grid, geometry, &smoothed);
                let first = route.points.len();
                route.points.extend(piece.points);
                route.portals.extend(piece.portals);
                if let (Some(tag), Some(slot)) = (after.take(), route.portals.get_mut(first)) {
                    *slot = Some(tag);
                }
                cells.cells.extend(smoothed.cells);
            }
            if let Some(link) = link {
                for node in &link.nodes {
                    route.points.push(node.position);
                    route.portals.push(node.tag);
                }
                after = link.after;
            }
            if let Some(exit) = exit {
                cells.hops.push(crate::search::PortalHop {
                    to: cells.cells.len(),
                    slot: hop.slot,
                });
                cells.cells.push(exit);
            }
            start = hop.to.saturating_add(1);
        }
        let tail: Vec<_> = path
            .cells
            .get(start..)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let tail = if tail.is_empty() {
            path.cells.last().copied().into_iter().collect()
        } else {
            tail
        };
        let edge = crate::smoothing::PortalEdge {
            into_portal: false,
            from_portal: after.is_some(),
        };
        let smoothed = smooth(tail, true, edge);
        let piece = self.route_of(grid, geometry, &smoothed);
        let first = route.points.len();
        route.points.extend(piece.points);
        route.portals.extend(piece.portals);
        if let (Some(tag), Some(slot)) = (after, route.portals.get_mut(first)) {
            *slot = Some(tag);
        }
        cells.cells.extend(smoothed.cells);
        (cells, route)
    }

    #[must_use]
    pub fn follow(
        &self,
        context: &MoveContext<'_>,
        state: MoverState,
        route: &Route,
        limits: &MotionLimits,
        dt: f32,
    ) -> Motion {
        self.follower.step(context, state, route, limits, dt)
    }

    #[must_use]
    pub fn avoid(
        &self,
        context: &MoveContext<'_>,
        agent: &Agent,
        origin: crate::geom::Vec2,
        motion: Motion,
        query: &dyn NeighbourQuery,
        scratch: &mut Vec<Neighbour>,
        dt: f32,
    ) -> Motion {
        Self::avoid_as(
            &*self.avoidance,
            context,
            agent,
            origin,
            motion,
            query,
            scratch,
            dt,
        )
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn avoid_with(
        &self,
        avoidance: &dyn Avoidance,
        context: &MoveContext<'_>,
        agent: &Agent,
        origin: crate::geom::Vec2,
        motion: Motion,
        query: &dyn NeighbourQuery,
        scratch: &mut Vec<Neighbour>,
        dt: f32,
    ) -> Motion {
        Self::avoid_as(
            avoidance, context, agent, origin, motion, query, scratch, dt,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn avoid_as(
        avoidance: &dyn Avoidance,
        context: &MoveContext<'_>,
        agent: &Agent,
        origin: crate::geom::Vec2,
        motion: Motion,
        query: &dyn NeighbourQuery,
        scratch: &mut Vec<Neighbour>,
        dt: f32,
    ) -> Motion {
        if motion.status == MotionStatus::Blocked {
            return motion;
        }
        let adjusted = avoidance.adjust(agent, motion.velocity, query, scratch);
        if adjusted == motion.velocity {
            return motion;
        }
        let position = origin.plus(adjusted.scale(dt));
        let passable = context
            .geometry
            .cell_at(position, context.layer, context.grid.extent())
            .is_some_and(|cell| context.grid.is_passable(cell, context.mover));
        if !passable {
            return Motion {
                state: MoverState {
                    speed: 0.0,
                    ..motion.state
                },
                velocity: crate::geom::Vec2::ZERO,
                status: MotionStatus::Blocked,
                phase: MotionPhase::NONE,
            };
        }
        Motion {
            state: MoverState {
                position,
                speed: adjusted.length(),
                ..motion.state
            },
            velocity: adjusted,
            status: MotionStatus::Moving,
            phase: motion.phase,
        }
    }

    #[must_use]
    pub fn avoidance_query_radius(&self, agent: &Agent) -> f32 {
        self.avoidance.query_radius(agent)
    }
}

#[cfg(test)]
mod tests;
