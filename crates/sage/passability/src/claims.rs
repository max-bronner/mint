use mint_movement::{Cell, CellFlags, CellGeometry, GridExtent, Passability, Vec2};

use crate::{FORD, GROUND, OBSTACLE};

pub const UNUSABLE: CellFlags = GROUND.blocked_by.union(FORD);

const UNUSABLE_UNDER_ITSELF: CellFlags = CellFlags::from_bits(UNUSABLE.bits() & !OBSTACLE.bits());

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClaimGrid {
    geometry: CellGeometry,
    extent: GridExtent,
}

impl ClaimGrid {
    #[must_use]
    pub fn new(cell_size: f32, ground: CellGeometry, ground_extent: GridExtent) -> Option<Self> {
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return None;
        }
        let geometry = CellGeometry::new(cell_size, ground.origin()).ok()?;
        let span = |cells: u16| {
            let cells = (f32::from(cells) * ground.cell_size() / cell_size).ceil();
            if cells.is_finite() && cells >= 1.0 && cells <= f32::from(u16::MAX) {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                Some(cells as u16)
            } else {
                None
            }
        };
        let extent =
            GridExtent::new(span(ground_extent.width)?, span(ground_extent.height)?).ok()?;
        Some(Self { geometry, extent })
    }

    #[must_use]
    pub fn cell_count(&self) -> usize {
        usize::try_from(self.extent.cell_count()).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claimer {
    pub key: u32,
    pub player: u32,
    pub finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hold {
    key: u32,
    player: u32,
}

const LEFT: Hold = Hold { key: 0, player: 0 };

#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum CellClaim {
    #[default]
    Free,
    Unfinished(Vec<Hold>),
    Finished(Hold),
}

impl CellClaim {
    fn holds_player(list: &[Hold], player: u32) -> bool {
        list.iter()
            .any(|hold| hold.key != 0 && hold.player == player)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimLayer {
    cells: Vec<CellClaim>,
}

impl ClaimLayer {
    #[must_use]
    pub fn empty(grid: &ClaimGrid) -> Self {
        Self {
            cells: vec![CellClaim::Free; grid.cell_count()],
        }
    }

    fn cell(&self, index: u32) -> Option<&CellClaim> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.cells.get(index))
    }

    #[must_use]
    pub fn holds(&self, index: u32, key: u32) -> bool {
        key != 0
            && match self.cell(index) {
                Some(CellClaim::Unfinished(list)) => list.iter().any(|hold| hold.key == key),
                Some(CellClaim::Finished(hold)) => hold.key == key,
                _ => false,
            }
    }

    pub fn take(&mut self, cells: &[u32], claimer: Claimer) -> u64 {
        if claimer.key == 0 {
            return 0;
        }
        let hold = Hold {
            key: claimer.key,
            player: claimer.player,
        };
        let mut taken = 0u64;
        for &index in cells {
            let Some(cell) = usize::try_from(index)
                .ok()
                .and_then(|index| self.cells.get_mut(index))
            else {
                continue;
            };
            let gained = match cell {
                CellClaim::Free => {
                    *cell = if claimer.finished {
                        CellClaim::Finished(hold)
                    } else {
                        CellClaim::Unfinished(vec![hold])
                    };
                    true
                }
                CellClaim::Unfinished(list) => {
                    if claimer.finished {
                        *cell = CellClaim::Finished(hold);
                        true
                    } else if CellClaim::holds_player(list, claimer.player) {
                        false
                    } else {
                        list.push(hold);
                        true
                    }
                }
                CellClaim::Finished(_) => {
                    if claimer.finished {
                        *cell = CellClaim::Finished(hold);
                    }
                    claimer.finished
                }
            };
            if gained {
                taken = taken.saturating_add(1);
            }
        }
        taken
    }

    pub fn release(&mut self, key: u32) {
        if key == 0 {
            return;
        }
        for cell in &mut self.cells {
            match cell {
                CellClaim::Unfinished(list) => {
                    if let Some(at) = list.iter().position(|hold| hold.key == key) {
                        if list.iter().take(at).all(|hold| hold.key == 0) {
                            *cell = CellClaim::Free;
                        } else if let Some(hold) = list.get_mut(at) {
                            *hold = LEFT;
                        }
                    }
                }
                CellClaim::Finished(hold) if hold.key == key => *cell = CellClaim::Free,
                _ => {}
            }
        }
    }

    pub fn rekey(&mut self, from: u32, to: u32) {
        if from == 0 || to == 0 {
            return;
        }
        for cell in &mut self.cells {
            match cell {
                CellClaim::Unfinished(list) => {
                    for hold in list.iter_mut().filter(|hold| hold.key == from) {
                        hold.key = to;
                    }
                }
                CellClaim::Finished(hold) if hold.key == from => hold.key = to,
                _ => {}
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub cells: Vec<u32>,
    pub wanted: u64,
}

impl Claim {
    #[must_use]
    pub fn share(&self) -> f32 {
        share(self.cells.len() as u64, self.wanted)
    }
}

#[must_use]
pub fn share(taken: u64, wanted: u64) -> f32 {
    if wanted == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let share = taken as f32 / wanted as f32;
    share.clamp(0.0, 1.0)
}

#[must_use]
pub fn assess(usability: &ClaimGround<'_>, centre: Vec2, radius: f32) -> Option<Claim> {
    let grid = usability.grid;
    let size = grid.geometry.cell_size();
    if !radius.is_finite() || radius <= 0.0 || radius / size > f32::from(u16::MAX) {
        return None;
    }
    let origin = grid.geometry.origin();
    let rows = cell_span(centre.y - radius, centre.y + radius, origin.y, size)?;
    let mut cells = Vec::new();
    let mut wanted = 0u64;
    let radius_sq = radius * radius;
    for row in rows.0..=rows.1 {
        #[allow(clippy::cast_precision_loss)]
        let dy = origin.y + (row as f32 + 0.5) * size - centre.y;
        let reach_sq = radius_sq - dy * dy;
        if reach_sq < 0.0 {
            continue;
        }
        let half = reach_sq.sqrt();
        let Some((low, high)) = cell_span(centre.x - half, centre.x + half, origin.x, size) else {
            continue;
        };
        wanted = wanted.saturating_add(u64::try_from(high.saturating_sub(low)).unwrap_or(0));
        wanted = wanted.saturating_add(1);
        let Ok(y) = u16::try_from(row) else {
            continue;
        };
        for column in low.max(0)..=high {
            let Ok(x) = u16::try_from(column) else {
                break;
            };
            let Some((index, usable)) = usability.cell(Cell::new(x, y)) else {
                break;
            };
            if usable {
                cells.push(index);
            }
        }
    }
    Some(Claim { cells, wanted })
}

pub struct ClaimGround<'a> {
    pub grid: &'a ClaimGrid,
    pub ground: &'a dyn Passability,
    pub ground_geometry: CellGeometry,
    pub layer: &'a ClaimLayer,
    pub claimer: Claimer,
    pub own: &'a [Cell],
    pub viewer: Option<Viewer<'a>>,
}

#[derive(Clone, Copy)]
pub struct Viewer<'a> {
    pub unexplored: &'a dyn Fn(Vec2) -> bool,
    pub fogged: &'a dyn Fn(Vec2) -> bool,
    pub hidden: &'a dyn Fn(u32) -> bool,
}

impl ClaimGround<'_> {
    fn cell(&self, cell: Cell) -> Option<(u32, bool)> {
        let index = self.grid.extent.index(cell)?;
        let centre = self.grid.geometry.centre_of(cell);
        let fogged = match &self.viewer {
            Some(viewer) if (viewer.unexplored)(centre) => return Some((index, false)),
            Some(viewer) => (viewer.fogged)(centre),
            None => false,
        };
        let claimable = match self.layer.cell(index) {
            None | Some(CellClaim::Free) => self.free_ground(centre),
            Some(CellClaim::Unfinished(list)) => {
                self.claimer.finished
                    || self.viewer.is_none()
                    || fogged
                    || !CellClaim::holds_player(list, self.claimer.player)
            }
            Some(CellClaim::Finished(hold)) => match &self.viewer {
                Some(viewer) => fogged || (viewer.hidden)(hold.key),
                None => self.claimer.finished && hold.key == self.claimer.key,
            },
        };
        Some((index, claimable))
    }

    fn free_ground(&self, centre: Vec2) -> bool {
        self.ground_geometry
            .cell_at(centre, 0, self.ground.extent())
            .is_some_and(|under| {
                let unusable = if self.own.contains(&under) {
                    UNUSABLE_UNDER_ITSELF
                } else {
                    UNUSABLE
                };
                !self.ground.flags(under).intersects(unusable)
            })
    }

    fn sample(&self, x: i64, y: i64) -> bool {
        let cell = |v: i64| u16::try_from(v.div_euclid(2)).ok();
        match (cell(x), cell(y)) {
            (Some(x), Some(y)) => self.cell(Cell::new(x, y)).is_some_and(|(_, usable)| usable),
            _ => false,
        }
    }
}

const FADE_KERNEL: [(i64, i64, f32); 12] = [
    (1, 0, 4.0),
    (0, 1, 4.0),
    (-1, 0, 4.0),
    (0, -1, 4.0),
    (1, 1, 2.0),
    (-1, -1, 2.0),
    (-1, 1, 3.0),
    (1, -1, 3.0),
    (2, 0, 1.0),
    (-2, 0, 1.0),
    (0, 2, 1.0),
    (0, -2, 1.0),
];

const FADE_FULL: f32 = 12.0;

const MAX_FADE_SIDE: f32 = 1024.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Fade {
    origin: Vec2,
    spacing: f32,
    columns: usize,
    weights: Vec<f32>,
}

impl Fade {
    #[must_use]
    pub fn at(&self, point: Vec2) -> f32 {
        let fx = (point.x - self.origin.x) / self.spacing;
        let fy = (point.y - self.origin.y) / self.spacing;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let weight = |dx: f32, dy: f32| {
            let (x, y) = (x0 + dx, y0 + dy);
            if !(x >= 0.0 && y >= 0.0 && x < MAX_FADE_SIDE && y < MAX_FADE_SIDE) {
                return 0.0;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (x, y) = (x as usize, y as usize);
            if x >= self.columns {
                return 0.0;
            }
            y.checked_mul(self.columns)
                .and_then(|row| row.checked_add(x))
                .and_then(|i| self.weights.get(i))
                .copied()
                .unwrap_or(0.0)
        };
        let low = weight(0.0, 0.0) + (weight(1.0, 0.0) - weight(0.0, 0.0)) * tx;
        let high = weight(0.0, 1.0) + (weight(1.0, 1.0) - weight(0.0, 1.0)) * tx;
        low + (high - low) * ty
    }
}

#[must_use]
pub fn fade(usability: &ClaimGround<'_>, centre: Vec2, radius: f32) -> Option<Fade> {
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let grid = usability.grid;
    let spacing = grid.geometry.cell_size() / 2.0;
    let origin = grid.geometry.origin();
    let span = |low: f32, high: f32, origin: f32| {
        let first = ((low - origin) / spacing).floor();
        let last = ((high - origin) / spacing).ceil();
        #[allow(clippy::cast_possible_truncation)]
        (first.is_finite() && last.is_finite() && last - first < MAX_FADE_SIDE)
            .then_some((first as i64, last as i64))
    };
    let (x0, x1) = span(centre.x - radius, centre.x + radius, origin.x)?;
    let (y0, y1) = span(centre.y - radius, centre.y + radius, origin.y)?;
    let columns = usize::try_from(x1.checked_sub(x0)?.checked_add(1)?).ok()?;
    let mut weights = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let weight = if usability.sample(x, y) {
                let lost: f32 = FADE_KERNEL
                    .iter()
                    .filter(|(dx, dy, _)| {
                        !usability.sample(x.saturating_add(*dx), y.saturating_add(*dy))
                    })
                    .map(|(_, _, weight)| weight)
                    .sum();
                ((FADE_FULL - lost) / FADE_FULL).clamp(0.0, 1.0)
            } else {
                0.0
            };
            weights.push(weight);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let origin = Vec2::new(
        origin.x + x0 as f32 * spacing,
        origin.y + y0 as f32 * spacing,
    );
    Some(Fade {
        origin,
        spacing,
        columns,
        weights,
    })
}

fn cell_span(low: f32, high: f32, origin: f32, size: f32) -> Option<(i64, i64)> {
    let first = ((low - origin) / size - 0.5).ceil();
    let last = ((high - origin) / size - 0.5).floor();
    if !first.is_finite() || !last.is_finite() || first > last {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some((first as i64, last as i64))
}

#[cfg(test)]
mod tests;
