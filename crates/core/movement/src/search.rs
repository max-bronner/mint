use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::grid::{Cell, GridExtent, MoverClass, Passability};
use crate::{Error, Result};

pub const COST_ORTHOGONAL: u32 = 10;

pub const COST_DIAGONAL: u32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct GoalId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathRequest {
    pub start: Cell,
    pub goal: Cell,
    pub mover: MoverClass,
    pub goal_id: GoalId,
    pub origin: Option<crate::Vec2>,
    pub corner_radius: f32,
    pub departure: Option<crate::Departure>,
    pub approach: Option<crate::Vec2>,
    pub prune: bool,
    pub final_goal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeBudget {
    limit: u32,
    spent: u32,
}

impl NodeBudget {
    #[must_use]
    pub const fn new(limit: u32) -> Self {
        Self { limit, spent: 0 }
    }

    #[must_use]
    pub const fn unlimited() -> Self {
        Self::new(u32::MAX)
    }

    #[must_use]
    pub const fn remaining(self) -> u32 {
        self.limit.saturating_sub(self.spent)
    }

    #[must_use]
    pub const fn spent(self) -> u32 {
        self.spent
    }

    pub fn spend(&mut self) -> bool {
        if self.spent >= self.limit {
            return false;
        }
        self.spent = self.spent.saturating_add(1);
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CellPath {
    pub cells: Vec<Cell>,
    pub cost: u32,
    pub hops: Vec<PortalHop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortalHop {
    pub to: usize,
    pub slot: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SearchOutcome {
    Found(CellPath),
    Unreachable,
    BudgetExhausted,
    Closest(CellPath),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchProgress {
    pub added: u32,
    pub open_empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub outcome: SearchOutcome,
    pub expanded: u32,
}

pub trait Search {
    fn find(
        &mut self,
        grid: &dyn Passability,
        request: &PathRequest,
        budget: &mut NodeBudget,
    ) -> Result<SearchResult>;

    fn find_closest(
        &mut self,
        grid: &dyn Passability,
        request: &PathRequest,
        budget: &mut NodeBudget,
    ) -> Result<SearchResult> {
        self.find(grid, request, budget)
    }

    fn find_closest_accepting(
        &mut self,
        grid: &dyn Passability,
        request: &PathRequest,
        budget: &mut NodeBudget,
        _accept: &dyn Fn(Cell) -> bool,
        _cost_weight: f32,
    ) -> Result<SearchResult> {
        self.find_closest(grid, request, budget)
    }
}

pub trait Heuristic {
    fn estimate(&self, from: Cell, to: Cell) -> u32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Octile {
    diagonal: u32,
}

impl Octile {
    #[must_use]
    pub const fn admissible() -> Self {
        Self {
            diagonal: COST_DIAGONAL.saturating_sub(COST_ORTHOGONAL),
        }
    }

    #[must_use]
    pub const fn new(diagonal: u32) -> Self {
        Self { diagonal }
    }
}

impl Default for Octile {
    fn default() -> Self {
        Self::admissible()
    }
}

impl Heuristic for Octile {
    fn estimate(&self, from: Cell, to: Cell) -> u32 {
        let dx = u32::from(from.x.abs_diff(to.x));
        let dy = u32::from(from.y.abs_diff(to.y));
        let (low, high) = if dx < dy { (dx, dy) } else { (dy, dx) };
        COST_ORTHOGONAL
            .saturating_mul(high)
            .saturating_add(self.diagonal.saturating_mul(low))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZeroHeuristic;

impl Heuristic for ZeroHeuristic {
    fn estimate(&self, _from: Cell, _to: Cell) -> u32 {
        0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct OpenKey {
    f: u32,
    h: u32,
    node: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct NodeState {
    stamp: u32,
    g: u32,
    parent: u32,
    closed: bool,
    via: u8,
    extra: u32,
}

const NEIGHBOURS: [(i32, i32); 8] = [
    (1, 0),
    (0, 1),
    (-1, 0),
    (0, -1),
    (1, 1),
    (-1, 1),
    (-1, -1),
    (1, -1),
];

const DIAGONAL_ORTHOGONALS: [(usize, usize); 4] = [(0, 1), (2, 1), (2, 3), (0, 3)];

#[derive(Debug, Clone)]
pub struct AStar<H> {
    heuristic: H,
    extent: GridExtent,
    nodes: Vec<NodeState>,
    open: BinaryHeap<Reverse<OpenKey>>,
    generation: u32,
}

impl AStar<Octile> {
    #[must_use]
    pub fn octile() -> Self {
        Self::new(Octile::admissible())
    }
}

impl<H: Heuristic> AStar<H> {
    #[must_use]
    pub fn new(heuristic: H) -> Self {
        Self {
            heuristic,
            extent: GridExtent::default(),
            nodes: Vec::new(),
            open: BinaryHeap::new(),
            generation: 0,
        }
    }

    pub const fn heuristic(&self) -> &H {
        &self.heuristic
    }

    fn begin(&mut self, extent: GridExtent) -> u32 {
        let count = usize::try_from(extent.cell_count()).unwrap_or(0);
        if self.extent != extent || self.nodes.len() != count {
            self.extent = extent;
            self.nodes.clear();
            self.nodes.resize(count, NodeState::default());
            self.generation = 0;
        }
        self.generation = match self.generation.checked_add(1) {
            Some(next) => next,
            None => {
                self.nodes
                    .iter_mut()
                    .for_each(|n| *n = NodeState::default());
                1
            }
        };
        self.open.clear();
        self.generation
    }

    fn state(&self, node: u32) -> Option<NodeState> {
        self.nodes.get(usize::try_from(node).ok()?).copied()
    }

    fn write(&mut self, node: u32, state: NodeState) {
        if let Ok(index) = usize::try_from(node) {
            if let Some(slot) = self.nodes.get_mut(index) {
                *slot = state;
            }
        }
    }

    fn reconstruct(&self, extent: GridExtent, start: u32, goal: u32) -> Option<CellPath> {
        let mut cells = Vec::new();
        let mut taken: Vec<(usize, u8)> = Vec::new();
        let mut node = goal;
        let mut steps = 0u32;
        let cap = extent.cell_count();
        loop {
            cells.push(extent.cell_at_index(node)?);
            if node == start {
                break;
            }
            steps = steps.saturating_add(1);
            if steps > cap {
                return None;
            }
            let state = self.state(node)?;
            if let Some(slot) = state.via.checked_sub(1) {
                taken.push((cells.len().saturating_sub(1), slot));
            }
            node = state.parent;
        }
        let last = cells.len().saturating_sub(1);
        cells.reverse();
        let hops = taken
            .into_iter()
            .rev()
            .map(|(at, slot)| PortalHop {
                to: last.saturating_sub(at),
                slot,
            })
            .collect();
        Some(CellPath {
            cells,
            cost: self.state(goal)?.g,
            hops,
        })
    }
}

fn cell_distance_squared(cell: Cell, goal: Cell) -> u64 {
    let dx = u64::from(cell.x.abs_diff(goal.x));
    let dy = u64::from(cell.y.abs_diff(goal.y));
    dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
}

type Screen = (u64, u32);

#[allow(clippy::cast_precision_loss)]
fn prune_skips(distance: u64, screen: Screen) -> bool {
    if distance < 100 {
        return false;
    }
    let (closest, walk) = screen;
    let threshold = if walk <= 25 {
        closest.saturating_mul(4) as f32
    } else {
        let reach = (closest as f32).sqrt() + 25.0;
        reach * reach
    };
    distance as f32 > threshold
}

fn cell_walk(cell: Cell, goal: Cell) -> u32 {
    cell.x
        .abs_diff(goal.x)
        .saturating_add(cell.y.abs_diff(goal.y))
        .into()
}

fn entering_cost(grid: &dyn Passability, cell: Cell, goal: Option<Cell>) -> u32 {
    grid.extra_cost(cell)
        .saturating_add(grid.goal_extra_cost(cell, goal))
}

const fn step_cost(extra: u32, diagonal: bool) -> u32 {
    let base = if diagonal {
        COST_DIAGONAL
    } else {
        COST_ORTHOGONAL
    };
    base.saturating_add(extra)
}

pub(crate) fn offset(cell: Cell, delta: (i32, i32), extent: GridExtent) -> Option<Cell> {
    let x = i32::from(cell.x).checked_add(delta.0)?;
    let y = i32::from(cell.y).checked_add(delta.1)?;
    let moved = Cell::on_layer(u16::try_from(x).ok()?, u16::try_from(y).ok()?, cell.layer);
    extent.contains(moved).then_some(moved)
}

impl<H: Heuristic> AStar<H> {
    fn run(
        &mut self,
        grid: &dyn Passability,
        request: &PathRequest,
        budget: &mut NodeBudget,
        want_closest: bool,
        accept: Option<&dyn Fn(Cell) -> bool>,
        cost_weight: f32,
    ) -> Result<SearchResult> {
        let extent = grid.extent();
        let start = extent.index(request.start).ok_or(Error::CellOutOfBounds {
            layer: request.start.layer,
            x: request.start.x,
            y: request.start.y,
        })?;
        let goal = extent.index(request.goal).ok_or(Error::CellOutOfBounds {
            layer: request.goal.layer,
            x: request.goal.x,
            y: request.goal.y,
        })?;

        if start == goal {
            return Ok(SearchResult {
                outcome: SearchOutcome::Found(CellPath {
                    cells: vec![request.start],
                    cost: 0,
                    hops: Vec::new(),
                }),
                expanded: 0,
            });
        }
        if !grid.is_passable(request.start, request.mover)
            || (!want_closest && !grid.is_passable(request.goal, request.mover))
        {
            return Ok(SearchResult {
                outcome: SearchOutcome::Unreachable,
                expanded: 0,
            });
        }

        let stamp = self.begin(extent);
        let mut expanded = 0u32;
        let mut closest: Option<(f32, u32)> = None;
        let goal_valid = grid.is_passable(request.goal, request.mover);
        let mut bar = f32::INFINITY;
        let mut screen: Screen = (
            cell_distance_squared(request.start, request.goal),
            cell_walk(request.start, request.goal),
        );
        self.write(
            start,
            NodeState {
                stamp,
                g: 0,
                parent: start,
                closed: false,
                via: 0,
                extra: entering_cost(grid, request.start, Some(request.goal)),
            },
        );
        let h0 = self.heuristic.estimate(request.start, request.goal);
        self.open.push(Reverse(OpenKey {
            f: h0,
            h: h0,
            node: start,
        }));

        while let Some(Reverse(entry)) = self.open.pop() {
            let Some(mut state) = self.state(entry.node) else {
                continue;
            };
            if state.stamp != stamp || state.closed {
                continue;
            }
            if !budget.spend() {
                let outcome = self.give_up(extent, start, closest, want_closest);
                return Ok(SearchResult {
                    outcome: outcome.unwrap_or(SearchOutcome::BudgetExhausted),
                    expanded,
                });
            }
            expanded = expanded.saturating_add(1);
            state.closed = true;
            self.write(entry.node, state);
            if want_closest && entry.node != start {
                if let Some(cell) = extent.cell_at_index(entry.node) {
                    #[allow(clippy::cast_precision_loss)]
                    let score = cell_distance_squared(cell, request.goal) as f32
                        + cost_weight * (state.g as f32) * (state.g as f32);
                    if score < bar {
                        let accepted = accept.is_none_or(|accept| accept(cell));
                        if goal_valid || accepted {
                            bar = score;
                        }
                        if accepted {
                            closest = Some((score, entry.node));
                        }
                    }
                }
            }

            if entry.node == goal {
                let path = self
                    .reconstruct(extent, start, goal)
                    .ok_or(Error::PathReconstruction)?;
                return Ok(SearchResult {
                    outcome: SearchOutcome::Found(path),
                    expanded,
                });
            }

            let Some(current) = extent.cell_at_index(entry.node) else {
                continue;
            };
            let mut orthogonal_open = [false; 4];
            for (slot, delta) in NEIGHBOURS.iter().enumerate() {
                let diagonal = slot >= 4;
                if diagonal {
                    let Some((a, b)) = DIAGONAL_ORTHOGONALS.get(slot.saturating_sub(4)).copied()
                    else {
                        continue;
                    };
                    let open = orthogonal_open.get(a).copied().unwrap_or(false)
                        || orthogonal_open.get(b).copied().unwrap_or(false);
                    if !open {
                        continue;
                    }
                }
                let Some(neighbour) = offset(current, *delta, extent) else {
                    continue;
                };
                let distance = cell_distance_squared(neighbour, request.goal);
                if distance < screen.0 {
                    screen = (distance, cell_walk(neighbour, request.goal));
                }
                if !grid.is_passable(neighbour, request.mover)
                    || !grid.admits_step(current, neighbour, request.mover)
                {
                    continue;
                }
                if let Some(slot) = orthogonal_open.get_mut(slot) {
                    *slot = true;
                }
                if request.prune && prune_skips(distance, screen) {
                    continue;
                }
                let Some(index) = extent.index(neighbour) else {
                    continue;
                };
                let existing = self.state(index).unwrap_or_default();
                let seen = existing.stamp == stamp;
                if seen && existing.closed {
                    continue;
                }
                let extra = if seen {
                    existing.extra
                } else {
                    entering_cost(grid, neighbour, Some(request.goal))
                };
                let tentative = state.g.saturating_add(step_cost(
                    extra.saturating_add(grid.step_extra_cost(current, neighbour)),
                    diagonal,
                ));
                if seen && existing.g <= tentative {
                    continue;
                }
                self.write(
                    index,
                    NodeState {
                        stamp,
                        g: tentative,
                        parent: entry.node,
                        closed: false,
                        via: 0,
                        extra,
                    },
                );
                let h = self.heuristic.estimate(neighbour, request.goal);
                self.open.push(Reverse(OpenKey {
                    f: tentative.saturating_add(h),
                    h,
                    node: index,
                }));
            }
            for link in grid.portal_links(current) {
                if !request.mover.portal_pass.has(link.slot) {
                    continue;
                }
                let _ = self.open_link(
                    grid,
                    extent,
                    stamp,
                    (entry.node, state.g),
                    link,
                    request.goal,
                    0,
                );
            }
        }

        let outcome = self.give_up(extent, start, closest, want_closest);
        Ok(SearchResult {
            outcome: outcome.unwrap_or(SearchOutcome::Unreachable),
            expanded,
        })
    }

    pub fn find_where(
        &mut self,
        grid: &dyn Passability,
        start: Cell,
        mover: MoverClass,
        bias_towards: Cell,
        floor: u32,
        budget: &mut NodeBudget,
        mut accept: impl FnMut(Cell) -> bool,
    ) -> Result<SearchResult> {
        self.find_where_with(
            grid,
            start,
            mover,
            bias_towards,
            floor,
            budget,
            |cell, _| accept(cell),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn find_where_with(
        &mut self,
        grid: &dyn Passability,
        start: Cell,
        mover: MoverClass,
        bias_towards: Cell,
        floor: u32,
        budget: &mut NodeBudget,
        mut accept: impl FnMut(Cell, SearchProgress) -> bool,
    ) -> Result<SearchResult> {
        let extent = grid.extent();
        let start_index = extent.index(start).ok_or(Error::CellOutOfBounds {
            layer: start.layer,
            x: start.x,
            y: start.y,
        })?;
        if !grid.is_passable(start, mover) {
            return Ok(SearchResult {
                outcome: SearchOutcome::Unreachable,
                expanded: 0,
            });
        }

        let stamp = self.begin(extent);
        let mut expanded = 0u32;
        let mut added = 0u32;
        let mut pending = 1u32;
        self.write(
            start_index,
            NodeState {
                stamp,
                g: 0,
                parent: start_index,
                closed: false,
                via: 0,
                extra: entering_cost(grid, start, None),
            },
        );
        let h0 = self
            .heuristic
            .estimate(start, bias_towards)
            .saturating_sub(floor);
        self.open.push(Reverse(OpenKey {
            f: h0,
            h: h0,
            node: start_index,
        }));

        while let Some(Reverse(entry)) = self.open.pop() {
            let Some(mut state) = self.state(entry.node) else {
                continue;
            };
            if state.stamp != stamp || state.closed {
                continue;
            }
            if !budget.spend() {
                return Ok(SearchResult {
                    outcome: SearchOutcome::BudgetExhausted,
                    expanded,
                });
            }
            expanded = expanded.saturating_add(1);
            state.closed = true;
            pending = pending.saturating_sub(1);
            self.write(entry.node, state);

            let Some(current) = extent.cell_at_index(entry.node) else {
                continue;
            };
            let progress = SearchProgress {
                added,
                open_empty: pending == 0,
            };
            if entry.node != start_index && accept(current, progress) {
                let path = self
                    .reconstruct(extent, start_index, entry.node)
                    .ok_or(Error::PathReconstruction)?;
                return Ok(SearchResult {
                    outcome: SearchOutcome::Found(path),
                    expanded,
                });
            }

            let mut orthogonal_open = [false; 4];
            for (slot, delta) in NEIGHBOURS.iter().enumerate() {
                let diagonal = slot >= 4;
                if diagonal {
                    let Some((a, b)) = DIAGONAL_ORTHOGONALS.get(slot.saturating_sub(4)).copied()
                    else {
                        continue;
                    };
                    let open = orthogonal_open.get(a).copied().unwrap_or(false)
                        || orthogonal_open.get(b).copied().unwrap_or(false);
                    if !open {
                        continue;
                    }
                }
                let Some(neighbour) = offset(current, *delta, extent) else {
                    continue;
                };
                if !grid.is_passable(neighbour, mover)
                    || !grid.admits_step(current, neighbour, mover)
                {
                    continue;
                }
                if let Some(slot) = orthogonal_open.get_mut(slot) {
                    *slot = true;
                }
                let Some(index) = extent.index(neighbour) else {
                    continue;
                };
                let existing = self.state(index).unwrap_or_default();
                let seen = existing.stamp == stamp;
                if seen && existing.closed {
                    continue;
                }
                let extra = if seen {
                    existing.extra
                } else {
                    entering_cost(grid, neighbour, None)
                };
                let tentative = state.g.saturating_add(step_cost(
                    extra.saturating_add(grid.step_extra_cost(current, neighbour)),
                    diagonal,
                ));
                if seen && existing.g <= tentative {
                    continue;
                }
                if !seen {
                    added = added.saturating_add(1);
                    pending = pending.saturating_add(1);
                }
                self.write(
                    index,
                    NodeState {
                        stamp,
                        g: tentative,
                        parent: entry.node,
                        closed: false,
                        via: 0,
                        extra,
                    },
                );
                let h = self
                    .heuristic
                    .estimate(neighbour, bias_towards)
                    .saturating_sub(floor);
                self.open.push(Reverse(OpenKey {
                    f: tentative.saturating_add(h),
                    h,
                    node: index,
                }));
            }
            for link in grid.portal_links(current) {
                if !mover.portal_pass.has(link.slot) {
                    continue;
                }
                if self.open_link(
                    grid,
                    extent,
                    stamp,
                    (entry.node, state.g),
                    link,
                    bias_towards,
                    floor,
                ) {
                    added = added.saturating_add(1);
                    pending = pending.saturating_add(1);
                }
            }
        }

        Ok(SearchResult {
            outcome: SearchOutcome::Unreachable,
            expanded,
        })
    }

    pub fn find_first_visit(
        &mut self,
        grid: &dyn Passability,
        request: &PathRequest,
        budget: &mut NodeBudget,
        refuse: &dyn Fn(Cell) -> bool,
    ) -> Result<SearchResult> {
        let extent = grid.extent();
        let start = extent.index(request.start).ok_or(Error::CellOutOfBounds {
            layer: request.start.layer,
            x: request.start.x,
            y: request.start.y,
        })?;
        let goal = extent.index(request.goal).ok_or(Error::CellOutOfBounds {
            layer: request.goal.layer,
            x: request.goal.x,
            y: request.goal.y,
        })?;
        if start == goal {
            return Ok(SearchResult {
                outcome: SearchOutcome::Found(CellPath {
                    cells: vec![request.start],
                    cost: 0,
                    hops: Vec::new(),
                }),
                expanded: 0,
            });
        }
        if !grid.is_passable(request.start, request.mover)
            || !grid.is_passable(request.goal, request.mover)
        {
            return Ok(SearchResult {
                outcome: SearchOutcome::Unreachable,
                expanded: 0,
            });
        }

        let stamp = self.begin(extent);
        let mut expanded = 0u32;
        self.write(
            start,
            NodeState {
                stamp,
                g: 0,
                parent: start,
                closed: false,
                via: 0,
                extra: entering_cost(grid, request.start, Some(request.goal)),
            },
        );
        let h0 = self.heuristic.estimate(request.start, request.goal);
        self.open.push(Reverse(OpenKey {
            f: h0,
            h: h0,
            node: start,
        }));

        while let Some(Reverse(entry)) = self.open.pop() {
            let Some(mut state) = self.state(entry.node) else {
                continue;
            };
            if state.stamp != stamp || state.closed {
                continue;
            }
            if !budget.spend() {
                return Ok(SearchResult {
                    outcome: SearchOutcome::BudgetExhausted,
                    expanded,
                });
            }
            expanded = expanded.saturating_add(1);
            state.closed = true;
            self.write(entry.node, state);
            if entry.node == goal {
                let path = self
                    .reconstruct(extent, start, goal)
                    .ok_or(Error::PathReconstruction)?;
                return Ok(SearchResult {
                    outcome: SearchOutcome::Found(path),
                    expanded,
                });
            }

            let Some(current) = extent.cell_at_index(entry.node) else {
                continue;
            };
            let mut orthogonal_open = [false; 4];
            for (slot, delta) in NEIGHBOURS.iter().enumerate() {
                let diagonal = slot >= 4;
                let Some(neighbour) = offset(current, *delta, extent) else {
                    continue;
                };
                if !grid.is_passable(neighbour, request.mover)
                    || !grid.admits_step(current, neighbour, request.mover)
                {
                    continue;
                }
                let Some(index) = extent.index(neighbour) else {
                    continue;
                };
                if self.state(index).is_some_and(|seen| seen.stamp == stamp) {
                    continue;
                }
                if index != goal {
                    if diagonal {
                        let Some((a, b)) =
                            DIAGONAL_ORTHOGONALS.get(slot.saturating_sub(4)).copied()
                        else {
                            continue;
                        };
                        let open = orthogonal_open.get(a).copied().unwrap_or(false)
                            || orthogonal_open.get(b).copied().unwrap_or(false);
                        if !open || refuse(neighbour) {
                            continue;
                        }
                    } else if refuse(neighbour) {
                        continue;
                    }
                }
                if let Some(flag) = orthogonal_open.get_mut(slot) {
                    *flag = true;
                }
                let extra = entering_cost(grid, neighbour, Some(request.goal));
                let tentative = state.g.saturating_add(step_cost(
                    extra.saturating_add(grid.step_extra_cost(current, neighbour)),
                    diagonal,
                ));
                self.write(
                    index,
                    NodeState {
                        stamp,
                        g: tentative,
                        parent: entry.node,
                        closed: false,
                        via: 0,
                        extra,
                    },
                );
                let h = self.heuristic.estimate(neighbour, request.goal);
                self.open.push(Reverse(OpenKey {
                    f: tentative.saturating_add(h),
                    h,
                    node: index,
                }));
            }
            for link in grid.portal_links(current) {
                if !request.mover.portal_pass.has(link.slot) {
                    continue;
                }
                let listed = extent
                    .index(link.exit)
                    .and_then(|exit| self.state(exit))
                    .is_some_and(|seen| seen.stamp == stamp);
                if !listed {
                    let _ = self.open_link(
                        grid,
                        extent,
                        stamp,
                        (entry.node, state.g),
                        link,
                        request.goal,
                        0,
                    );
                }
            }
        }
        Ok(SearchResult {
            outcome: SearchOutcome::Unreachable,
            expanded,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn open_link(
        &mut self,
        grid: &dyn Passability,
        extent: GridExtent,
        stamp: u32,
        from: (u32, u32),
        link: &crate::grid::PortalLink,
        towards: Cell,
        floor: u32,
    ) -> bool {
        let Some(index) = extent.index(link.exit) else {
            return false;
        };
        let existing = self.state(index).unwrap_or_default();
        let seen = existing.stamp == stamp;
        if seen && existing.closed {
            return false;
        }
        let extra = if seen {
            existing.extra
        } else {
            entering_cost(grid, link.exit, Some(towards))
        };
        let tentative = from.1.saturating_add(step_cost(extra, false));
        if seen && existing.g <= tentative {
            return false;
        }
        self.write(
            index,
            NodeState {
                stamp,
                g: tentative,
                parent: from.0,
                closed: false,
                via: link.slot.saturating_add(1),
                extra,
            },
        );
        let h = self
            .heuristic
            .estimate(link.exit, towards)
            .saturating_sub(floor);
        self.open.push(Reverse(OpenKey {
            f: tentative.saturating_add(h),
            h,
            node: index,
        }));
        !seen
    }

    fn give_up(
        &self,
        extent: GridExtent,
        start: u32,
        closest: Option<(f32, u32)>,
        want_closest: bool,
    ) -> Option<SearchOutcome> {
        if !want_closest {
            return None;
        }
        let (_, node) = closest?;
        Some(SearchOutcome::Closest(
            self.reconstruct(extent, start, node)?,
        ))
    }
}

impl<H: Heuristic> Search for AStar<H> {
    fn find(
        &mut self,
        grid: &dyn Passability,
        request: &PathRequest,
        budget: &mut NodeBudget,
    ) -> Result<SearchResult> {
        self.run(grid, request, budget, false, None, 0.0)
    }

    fn find_closest(
        &mut self,
        grid: &dyn Passability,
        request: &PathRequest,
        budget: &mut NodeBudget,
    ) -> Result<SearchResult> {
        self.run(grid, request, budget, true, None, 0.0)
    }

    fn find_closest_accepting(
        &mut self,
        grid: &dyn Passability,
        request: &PathRequest,
        budget: &mut NodeBudget,
        accept: &dyn Fn(Cell) -> bool,
        cost_weight: f32,
    ) -> Result<SearchResult> {
        self.run(grid, request, budget, true, Some(accept), cost_weight)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StraightLineSearch;

impl Search for StraightLineSearch {
    fn find(
        &mut self,
        grid: &dyn Passability,
        request: &PathRequest,
        _budget: &mut NodeBudget,
    ) -> Result<SearchResult> {
        let extent = grid.extent();
        for cell in [request.start, request.goal] {
            if !extent.contains(cell) {
                return Err(Error::CellOutOfBounds {
                    layer: cell.layer,
                    x: cell.x,
                    y: cell.y,
                });
            }
        }
        let cells = if request.start == request.goal {
            vec![request.start]
        } else {
            vec![request.start, request.goal]
        };
        Ok(SearchResult {
            outcome: SearchOutcome::Found(CellPath {
                cost: Octile::admissible().estimate(request.start, request.goal),
                cells,
                hops: Vec::new(),
            }),
            expanded: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{CellFlags, CostGrid};

    const WALL: CellFlags = CellFlags::from_bits(1);

    fn open_grid(width: u16, height: u16) -> CostGrid {
        CostGrid::new(GridExtent::new(width, height).unwrap())
    }

    fn request(start: Cell, goal: Cell) -> PathRequest {
        PathRequest {
            corner_radius: 0.0,
            departure: None,
            approach: None,
            prune: false,
            final_goal: true,
            start,
            goal,
            mover: MoverClass::new(WALL),
            goal_id: GoalId(0),
            origin: None,
        }
    }

    fn found(result: SearchResult) -> CellPath {
        match result.outcome {
            SearchOutcome::Found(path) => path,
            other => panic!("expected a path, got {other:?}"),
        }
    }

    #[test]
    fn a_pruned_search_for_a_walled_off_goal_expands_fewer_cells() {
        let mut grid = open_grid(80, 80);
        for y in 0..80 {
            grid.set_flags(Cell::new(60, y), WALL).unwrap();
        }
        let expanded = |prune: bool| {
            let mut asked = request(Cell::new(10, 40), Cell::new(70, 40));
            asked.prune = prune;
            let mut search = AStar::octile();
            let result = search
                .find(&grid, &asked, &mut NodeBudget::unlimited())
                .unwrap();
            assert_eq!(result.outcome, SearchOutcome::Unreachable);
            result.expanded
        };
        assert!(expanded(true) < expanded(false) / 2);
    }

    #[test]
    fn the_open_set_key_is_a_total_order_over_distinct_nodes() {
        let a = OpenKey {
            f: 10,
            h: 4,
            node: 7,
        };
        let b = OpenKey {
            f: 10,
            h: 4,
            node: 8,
        };
        assert!(a < b);
        assert_ne!(a, b);
        assert!(
            OpenKey {
                f: 10,
                h: 3,
                node: 9
            } < a,
            "lower h must win at equal f"
        );
    }

    #[test]
    fn a_path_across_open_ground_costs_the_octile_distance() {
        let grid = open_grid(16, 16);
        let mut search = AStar::octile();
        let result = search
            .find(
                &grid,
                &request(Cell::new(1, 1), Cell::new(9, 5)),
                &mut NodeBudget::unlimited(),
            )
            .unwrap();
        let path = found(result);
        assert_eq!(path.cost, 4 * 14 + 4 * 10);
        assert_eq!(path.cells.first(), Some(&Cell::new(1, 1)));
        assert_eq!(path.cells.last(), Some(&Cell::new(9, 5)));
    }

    #[test]
    fn a_wall_with_no_gap_is_reported_unreachable() {
        let mut grid = open_grid(8, 8);
        for y in 0..8 {
            grid.set_flags(Cell::new(4, y), WALL).unwrap();
        }
        let mut search = AStar::octile();
        let result = search
            .find(
                &grid,
                &request(Cell::new(1, 1), Cell::new(6, 6)),
                &mut NodeBudget::unlimited(),
            )
            .unwrap();
        assert_eq!(result.outcome, SearchOutcome::Unreachable);
    }

    #[test]
    fn a_goal_behind_a_wall_yields_a_path_to_the_cell_nearest_it() {
        let mut grid = open_grid(8, 8);
        for y in 0..8 {
            grid.set_flags(Cell::new(4, y), WALL).unwrap();
        }
        let mut search = AStar::octile();
        let result = search
            .find_closest(
                &grid,
                &request(Cell::new(1, 1), Cell::new(6, 6)),
                &mut NodeBudget::unlimited(),
            )
            .unwrap();
        let SearchOutcome::Closest(path) = result.outcome else {
            panic!("expected a closest path, got {:?}", result.outcome)
        };
        assert_eq!(path.cells.first(), Some(&Cell::new(1, 1)));
        assert_eq!(path.cells.last(), Some(&Cell::new(3, 6)));
    }

    #[test]
    fn a_closest_search_aimed_at_a_blocked_goal_ends_beside_it() {
        let mut grid = open_grid(8, 8);
        for y in 0..8 {
            for x in 5..8 {
                grid.set_flags(Cell::new(x, y), WALL).unwrap();
            }
        }
        let mut search = AStar::octile();
        let result = search
            .find_closest(
                &grid,
                &request(Cell::new(1, 3), Cell::new(6, 3)),
                &mut NodeBudget::unlimited(),
            )
            .unwrap();
        let SearchOutcome::Closest(path) = result.outcome else {
            panic!("expected a closest path, got {:?}", result.outcome)
        };
        assert_eq!(path.cells.last(), Some(&Cell::new(4, 3)));
    }

    #[test]
    fn a_cost_weight_trades_distance_to_the_goal_against_cost_so_far() {
        let mut grid = open_grid(10, 1);
        grid.set_flags(Cell::new(9, 0), WALL).unwrap();
        let end = |weight: f32| {
            let result = AStar::octile()
                .find_closest_accepting(
                    &grid,
                    &request(Cell::new(0, 0), Cell::new(9, 0)),
                    &mut NodeBudget::unlimited(),
                    &|_| true,
                    weight,
                )
                .unwrap();
            let SearchOutcome::Closest(path) = result.outcome else {
                panic!("expected a closest path, got {:?}", result.outcome)
            };
            path.cells.last().copied()
        };
        assert_eq!(end(0.0), Some(Cell::new(8, 0)));
        assert_eq!(end(1.0), Some(Cell::new(1, 0)));
    }

    #[test]
    fn a_unit_cannot_cut_the_corner_between_two_blocked_cells() {
        let mut grid = open_grid(3, 3);
        grid.set_flags(Cell::new(1, 0), WALL).unwrap();
        grid.set_flags(Cell::new(0, 1), WALL).unwrap();
        let mut search = AStar::octile();
        let result = search
            .find(
                &grid,
                &request(Cell::new(0, 0), Cell::new(1, 1)),
                &mut NodeBudget::unlimited(),
            )
            .unwrap();
        assert_eq!(result.outcome, SearchOutcome::Unreachable);
    }

    #[test]
    fn the_first_visit_search_never_relaxes_a_listed_cell() {
        let mut grid = open_grid(4, 3);
        grid.set_extra_cost(Cell::new(1, 1), 6).unwrap();
        let ask = request(Cell::new(0, 1), Cell::new(3, 1));
        let relaxed = found(
            AStar::new(ZeroHeuristic)
                .find(&grid, &ask, &mut NodeBudget::unlimited())
                .unwrap(),
        );
        let first = found(
            AStar::new(ZeroHeuristic)
                .find_first_visit(&grid, &ask, &mut NodeBudget::unlimited(), &|_| false)
                .unwrap(),
        );
        assert_eq!(relaxed.cost, 36);
        assert_eq!(first.cost, 38);
    }

    #[test]
    fn a_pinched_diagonal_with_one_side_open_is_allowed() {
        let mut grid = open_grid(3, 3);
        grid.set_flags(Cell::new(1, 0), WALL).unwrap();
        let mut search = AStar::octile();
        let result = search
            .find(
                &grid,
                &request(Cell::new(0, 0), Cell::new(1, 1)),
                &mut NodeBudget::unlimited(),
            )
            .unwrap();
        assert_eq!(found(result).cost, COST_DIAGONAL);
    }

    #[test]
    fn an_expensive_cell_is_walked_around_when_the_detour_is_cheaper() {
        let mut grid = open_grid(8, 3);
        for y in 0..3 {
            grid.set_extra_cost(Cell::new(4, y), 500).unwrap();
        }
        grid.set_extra_cost(Cell::new(4, 0), 0).unwrap();
        let mut search = AStar::octile();
        let result = search
            .find(
                &grid,
                &request(Cell::new(0, 1), Cell::new(7, 1)),
                &mut NodeBudget::unlimited(),
            )
            .unwrap();
        let path = found(result);
        assert!(
            path.cells.contains(&Cell::new(4, 0)),
            "the cheap gap should be used: {:?}",
            path.cells
        );
    }

    struct CostlyFarFromGoal(CostGrid);

    impl Passability for CostlyFarFromGoal {
        fn extent(&self) -> GridExtent {
            self.0.extent()
        }
        fn flags(&self, cell: Cell) -> CellFlags {
            self.0.flags(cell)
        }
        fn extra_cost(&self, cell: Cell) -> u32 {
            self.0.extra_cost(cell)
        }
        fn goal_extra_cost(&self, cell: Cell, goal: Option<Cell>) -> u32 {
            let near = goal.is_some_and(|goal| cell.x.abs_diff(goal.x) <= 3);
            if cell.y == 1 && cell.x == 4 && !near {
                500
            } else {
                0
            }
        }
    }

    #[test]
    fn a_goal_dependent_charge_steers_the_path_and_is_free_near_the_goal() {
        let grid = CostlyFarFromGoal(open_grid(12, 3));
        let mut search = AStar::octile();
        let far = found(
            search
                .find(
                    &grid,
                    &request(Cell::new(0, 1), Cell::new(11, 1)),
                    &mut NodeBudget::unlimited(),
                )
                .unwrap(),
        );
        assert!(!far.cells.contains(&Cell::new(4, 1)), "{:?}", far.cells);
        let near = found(
            search
                .find(
                    &grid,
                    &request(Cell::new(0, 1), Cell::new(6, 1)),
                    &mut NodeBudget::unlimited(),
                )
                .unwrap(),
        );
        assert!(near.cells.contains(&Cell::new(4, 1)), "{:?}", near.cells);
    }

    #[test]
    fn a_spent_budget_stops_the_search_rather_than_the_frame() {
        let grid = open_grid(64, 64);
        let mut search = AStar::octile();
        let mut budget = NodeBudget::new(4);
        let result = search
            .find(
                &grid,
                &request(Cell::new(0, 0), Cell::new(63, 63)),
                &mut budget,
            )
            .unwrap();
        assert_eq!(result.outcome, SearchOutcome::BudgetExhausted);
        assert_eq!(budget.remaining(), 0);
        assert!(result.expanded <= 4);
    }

    #[test]
    fn one_budget_across_two_searches_is_spent_by_both() {
        let grid = open_grid(32, 32);
        let mut search = AStar::octile();
        let mut budget = NodeBudget::new(200);
        let first = search
            .find(
                &grid,
                &request(Cell::new(0, 0), Cell::new(31, 31)),
                &mut budget,
            )
            .unwrap();
        let after_first = budget.spent();
        assert!(after_first > 0);
        let second = search
            .find(
                &grid,
                &request(Cell::new(0, 31), Cell::new(31, 0)),
                &mut budget,
            )
            .unwrap();
        assert_eq!(budget.spent(), after_first.saturating_add(second.expanded));
        assert!(matches!(first.outcome, SearchOutcome::Found(_)) || budget.remaining() == 0);
    }

    #[test]
    fn a_request_off_the_grid_is_an_error_not_a_panic() {
        let grid = open_grid(4, 4);
        let mut search = AStar::octile();
        let err = search
            .find(
                &grid,
                &request(Cell::new(0, 0), Cell::new(9, 9)),
                &mut NodeBudget::unlimited(),
            )
            .unwrap_err();
        assert!(matches!(err, Error::CellOutOfBounds { x: 9, y: 9, .. }));
    }

    #[test]
    fn the_null_search_returns_the_two_endpoints_and_ignores_the_wall() {
        let mut grid = open_grid(8, 8);
        for y in 0..8 {
            grid.set_flags(Cell::new(4, y), WALL).unwrap();
        }
        let mut search = StraightLineSearch;
        let result = search
            .find(
                &grid,
                &request(Cell::new(0, 0), Cell::new(7, 7)),
                &mut NodeBudget::unlimited(),
            )
            .unwrap();
        assert_eq!(found(result).cells, vec![Cell::new(0, 0), Cell::new(7, 7)]);
    }

    #[test]
    fn the_same_request_twice_gives_the_identical_path() {
        let mut grid = open_grid(24, 24);
        for y in 0..20 {
            grid.set_flags(Cell::new(10, y), WALL).unwrap();
        }
        let mut search = AStar::octile();
        let req = request(Cell::new(1, 1), Cell::new(22, 22));
        let first = search
            .find(&grid, &req, &mut NodeBudget::unlimited())
            .unwrap();
        let second = search
            .find(&grid, &req, &mut NodeBudget::unlimited())
            .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn equal_cost_alternatives_resolve_the_same_way_every_run() {
        let grid = open_grid(9, 9);
        let mut first: Option<Vec<Cell>> = None;
        for _ in 0..8 {
            let mut search = AStar::octile();
            let path = found(
                search
                    .find(
                        &grid,
                        &request(Cell::new(4, 0), Cell::new(4, 8)),
                        &mut NodeBudget::unlimited(),
                    )
                    .unwrap(),
            );
            match &first {
                None => first = Some(path.cells),
                Some(expected) => assert_eq!(&path.cells, expected),
            }
        }
    }

    #[test]
    fn find_where_with_reports_an_exhausted_open_list_on_the_last_cell() {
        let grid = open_grid(3, 3);
        let mut search = AStar::octile();
        let mut taken = Vec::new();
        let result = search
            .find_where_with(
                &grid,
                Cell::new(1, 1),
                MoverClass::new(WALL),
                Cell::new(1, 1),
                u32::MAX,
                &mut NodeBudget::unlimited(),
                |cell, progress| {
                    taken.push((cell, progress));
                    progress.open_empty && progress.added > 0
                },
            )
            .unwrap();
        let path = found(result);
        assert_eq!(
            taken.len(),
            8,
            "every other cell was tested before the search ran dry"
        );
        assert_eq!(path.cells.last(), taken.last().map(|(cell, _)| cell));
        assert_eq!(taken.last().map(|(_, progress)| progress.added), Some(8));
        assert!(taken
            .iter()
            .rev()
            .skip(1)
            .all(|(_, progress)| !progress.open_empty));
    }

    #[test]
    fn find_where_never_accepts_the_start_cell() {
        let grid = open_grid(3, 3);
        let mut search = AStar::octile();
        let result = search
            .find_where(
                &grid,
                Cell::new(1, 1),
                MoverClass::new(WALL),
                Cell::new(1, 1),
                0,
                &mut NodeBudget::unlimited(),
                |_| true,
            )
            .unwrap();
        let path = found(result);
        assert_eq!(path.cells.len(), 2);
        assert_ne!(path.cells.last(), Some(&Cell::new(1, 1)));
    }

    #[test]
    fn find_where_routes_around_a_wall_to_reach_an_accepted_cell() {
        let mut grid = open_grid(8, 8);
        for y in 1..8 {
            grid.set_flags(Cell::new(4, y), WALL).unwrap();
        }
        let mut search = AStar::octile();
        let target = Cell::new(6, 3);
        let result = search
            .find_where(
                &grid,
                Cell::new(0, 3),
                MoverClass::new(WALL),
                target,
                0,
                &mut NodeBudget::unlimited(),
                |cell| cell == target,
            )
            .unwrap();
        let path = found(result);
        assert_eq!(path.cells.last(), Some(&target));
        assert!(path.cells.contains(&Cell::new(4, 0)));
    }

    #[test]
    fn find_where_reports_budget_exhausted_rather_than_guessing() {
        let grid = open_grid(64, 64);
        let mut search = AStar::octile();
        let mut budget = NodeBudget::new(2);
        let result = search
            .find_where(
                &grid,
                Cell::new(0, 0),
                MoverClass::new(WALL),
                Cell::new(63, 63),
                0,
                &mut budget,
                |_| false,
            )
            .unwrap();
        assert_eq!(result.outcome, SearchOutcome::BudgetExhausted);
    }
}
