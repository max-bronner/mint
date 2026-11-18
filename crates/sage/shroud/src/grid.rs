use mint_sim::{HashedVec, PlayerId, StateHasher};

use crate::disc::DiscreteCircle;
use crate::sighting::{Sector, Sighting};

pub const MAX_PLAYERS: u32 = 16;

pub const MAX_GRID_SLOTS: usize = 16_777_216;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellVisibility {
    Unexplored,
    Explored,
    Visible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub struct PlayerMask(u16);

impl PlayerMask {
    pub const NONE: Self = Self(0);

    pub const ALL: Self = Self(u16::MAX);

    #[must_use]
    pub fn of(player: PlayerId) -> Self {
        Self(bit_of(player).unwrap_or(0))
    }

    #[must_use]
    pub fn holds(self, player: PlayerId) -> bool {
        bit_of(player).is_some_and(|bit| self.0 & bit != 0)
    }

    #[must_use]
    pub fn with(self, player: PlayerId) -> Self {
        Self(self.0 | bit_of(player).unwrap_or(0))
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u16(self.0);
    }
}

fn bit_of(player: PlayerId) -> Option<u16> {
    let index = u32::from(player.get());
    (index < MAX_PLAYERS).then(|| 1u16.checked_shl(index).unwrap_or(0))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridExtent {
    pub low_x: f32,
    pub low_y: f32,
    pub cell_size: f32,
    pub cells_x: u32,
    pub cells_y: u32,
}

impl GridExtent {
    #[must_use]
    pub fn covering(low: (f32, f32), high: (f32, f32), cell_size: f32) -> Self {
        let cell_size = if cell_size.is_finite() && cell_size > 0.0 {
            cell_size
        } else {
            1.0
        };
        let span = |lo: f32, hi: f32| -> u32 {
            let extent = hi - lo;
            if !extent.is_finite() || extent <= 0.0 {
                return 1;
            }
            let cells = (extent / cell_size).ceil();
            if (1.0..=65_536.0).contains(&cells) {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let whole = cells as u32;
                whole
            } else if cells > 65_536.0 {
                65_536
            } else {
                1
            }
        };
        Self {
            low_x: if low.0.is_finite() { low.0 } else { 0.0 },
            low_y: if low.1.is_finite() { low.1 } else { 0.0 },
            cell_size,
            cells_x: span(low.0, high.0),
            cells_y: span(low.1, high.1),
        }
    }

    #[must_use]
    pub fn contains(&self, world_x: f32, world_y: f32) -> bool {
        let x = axis_to_cell(world_x, self.low_x, self.cell_size);
        let y = axis_to_cell(world_y, self.low_y, self.cell_size);
        u32::try_from(x).is_ok_and(|x| x < self.cells_x)
            && u32::try_from(y).is_ok_and(|y| y < self.cells_y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("a visibility grid holds at most {MAX_PLAYERS} players, {asked} were asked for")]
    TooManyPlayers { asked: u32 },
    #[error(
        "a {cells_x}x{cells_y} grid for {players} players is {slots} counter slots, past the \
         {MAX_GRID_SLOTS} cap"
    )]
    GridTooLarge {
        cells_x: u32,
        cells_y: u32,
        players: u32,
        slots: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisibilityGrid {
    extent: GridExtent,
    players: u32,
    current: HashedVec<i32>,
    active: HashedVec<i32>,
    window: Option<(u32, u32)>,
}

impl VisibilityGrid {
    pub fn new(extent: GridExtent, players: u32) -> Result<Self, Error> {
        if players > MAX_PLAYERS {
            return Err(Error::TooManyPlayers { asked: players });
        }
        let slots = u64::from(extent.cells_x)
            .saturating_mul(u64::from(extent.cells_y))
            .saturating_mul(u64::from(players));
        let capped = usize::try_from(slots).unwrap_or(usize::MAX);
        if capped > MAX_GRID_SLOTS {
            return Err(Error::GridTooLarge {
                cells_x: extent.cells_x,
                cells_y: extent.cells_y,
                players,
                slots,
            });
        }
        Ok(Self {
            extent,
            players,
            current: HashedVec::from(vec![1; capped]),
            active: HashedVec::from(vec![0; capped]),
            window: None,
        })
    }

    pub fn set_window(&mut self, high: Option<(f32, f32)>) {
        self.window = high.map(|(x, y)| {
            let covering = GridExtent::covering((0.0, 0.0), (x, y), self.extent.cell_size);
            (covering.cells_x, covering.cells_y)
        });
    }

    #[must_use]
    pub const fn extent(&self) -> GridExtent {
        self.extent
    }

    #[must_use]
    pub fn contains(&self, world_x: f32, world_y: f32) -> bool {
        self.extent.contains(world_x, world_y)
    }

    #[must_use]
    pub const fn players(&self) -> u32 {
        self.players
    }

    #[must_use]
    pub const fn cell_count(&self) -> u64 {
        (self.extent.cells_x as u64).saturating_mul(self.extent.cells_y as u64)
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn map_diagonal(&self) -> f32 {
        let width = self.extent.cells_x as f32 * self.extent.cell_size;
        let height = self.extent.cells_y as f32 * self.extent.cell_size;
        (width * width + height * height).sqrt()
    }

    #[must_use]
    pub fn world_to_cell(&self, world_x: f32, world_y: f32) -> (i32, i32) {
        (
            axis_to_cell(world_x, self.extent.low_x, self.extent.cell_size),
            axis_to_cell(world_y, self.extent.low_y, self.extent.cell_size),
        )
    }

    #[must_use]
    pub fn world_to_cell_dist(&self, distance: f32) -> i32 {
        if !distance.is_finite() || self.extent.cell_size <= 0.0 {
            return 0;
        }
        let cells = (distance / self.extent.cell_size).ceil();
        if cells <= 0.0 {
            0
        } else if cells >= f32::from(i16::MAX) {
            i32::from(i16::MAX)
        } else {
            #[allow(clippy::cast_possible_truncation)]
            let whole = cells as i32;
            whole
        }
    }

    fn slot(&self, player: PlayerId, x: i32, y: i32) -> Option<usize> {
        let index = u32::from(player.get());
        if index >= self.players {
            return None;
        }
        let (cells_x, cells_y) = (self.extent.cells_x, self.extent.cells_y);
        let column = u32::try_from(x).ok().filter(|c| *c < cells_x)?;
        let row = u32::try_from(y).ok().filter(|r| *r < cells_y)?;
        let cell = u64::from(row)
            .checked_mul(u64::from(cells_x))?
            .checked_add(u64::from(column))?;
        let slot = cell
            .checked_mul(u64::from(self.players))?
            .checked_add(u64::from(index))?;
        usize::try_from(slot).ok()
    }

    #[must_use]
    pub fn cell_state(&self, player: PlayerId, x: i32, y: i32) -> CellVisibility {
        let Some(slot) = self.slot(player, x, y) else {
            return CellVisibility::Unexplored;
        };
        if self.window.is_some_and(|(columns, rows)| {
            u32::try_from(x).map_or(true, |x| x >= columns)
                || u32::try_from(y).map_or(true, |y| y >= rows)
        }) {
            return CellVisibility::Unexplored;
        }
        match self.current.get(slot).copied().unwrap_or(1) {
            1 => CellVisibility::Unexplored,
            0 => CellVisibility::Explored,
            _ => CellVisibility::Visible,
        }
    }

    #[must_use]
    pub fn looker_count(&self, player: PlayerId, x: i32, y: i32) -> u32 {
        let Some(slot) = self.slot(player, x, y) else {
            return 0;
        };
        let current = self.current.get(slot).copied().unwrap_or(1);
        if current < 0 {
            current.unsigned_abs()
        } else {
            0
        }
    }

    #[must_use]
    pub fn shrouder_count(&self, player: PlayerId, x: i32, y: i32) -> i32 {
        self.slot(player, x, y)
            .and_then(|slot| self.active.get(slot).copied())
            .unwrap_or(0)
    }

    pub fn add_looker(&mut self, player: PlayerId, x: i32, y: i32) {
        let Some(slot) = self.slot(player, x, y) else {
            return;
        };
        if let Some(current) = self.current.get_mut(slot) {
            *current = current.saturating_sub(1).min(-1);
        }
    }

    pub fn remove_looker(&mut self, player: PlayerId, x: i32, y: i32) {
        let Some(slot) = self.slot(player, x, y) else {
            return;
        };
        let active = self.active.get(slot).copied().unwrap_or(0);
        if let Some(current) = self.current.get_mut(slot) {
            if *current == -1 {
                *current = active.min(1);
            } else {
                *current = current.saturating_add(1);
            }
        }
    }

    pub fn add_shrouder(&mut self, player: PlayerId, x: i32, y: i32) {
        let Some(slot) = self.slot(player, x, y) else {
            return;
        };
        if let Some(active) = self.active.get_mut(slot) {
            *active = active.saturating_add(1);
        }
        if let Some(current) = self.current.get_mut(slot) {
            if *current == 0 {
                *current = 1;
            }
        }
    }

    pub fn remove_shrouder(&mut self, player: PlayerId, x: i32, y: i32) {
        let Some(slot) = self.slot(player, x, y) else {
            return;
        };
        if let Some(active) = self.active.get_mut(slot) {
            *active = active.saturating_sub(1);
        }
    }

    pub fn reveal(&mut self, world_x: f32, world_y: f32, radius: f32, mask: PlayerMask) {
        self.apply_disc(world_x, world_y, radius, mask, Self::add_looker);
    }

    pub fn unreveal(&mut self, world_x: f32, world_y: f32, radius: f32, mask: PlayerMask) {
        self.apply_disc(world_x, world_y, radius, mask, Self::remove_looker);
    }

    pub fn reveal_sighting(&mut self, sighting: &Sighting) {
        self.apply_sighting(sighting, Self::add_looker);
    }

    pub fn unreveal_sighting(&mut self, sighting: &Sighting) {
        self.apply_sighting(sighting, Self::remove_looker);
    }

    fn apply_sighting(
        &mut self,
        sighting: &Sighting,
        operation: fn(&mut Self, PlayerId, i32, i32),
    ) {
        let shape = sighting.shape;
        if shape.is_round() {
            self.apply_disc(
                sighting.world_x,
                sighting.world_y,
                sighting.radius,
                sighting.mask,
                operation,
            );
            return;
        }
        let (centre_x, centre_y) = self.world_to_cell(sighting.world_x, sighting.world_y);
        for sector in [Sector::Front, Sector::Side, Sector::Rear] {
            let reach = sighting.radius * shape.fraction_of(sector);
            let cell_radius = match sector {
                Sector::Front => self.world_to_cell_dist(reach).max(1),
                Sector::Side | Sector::Rear => self.world_to_cell_dist(reach),
            };
            let circle = DiscreteCircle::new(centre_x, centre_y, cell_radius);
            self.apply_circle(&circle, sighting.mask, operation, |x, y| {
                #[allow(clippy::cast_precision_loss)]
                let (dx, dy) = (
                    x.saturating_sub(centre_x) as f32,
                    y.saturating_sub(centre_y) as f32,
                );
                shape.sector(dx, dy) == sector
            });
        }
    }

    fn apply_disc(
        &mut self,
        world_x: f32,
        world_y: f32,
        radius: f32,
        mask: PlayerMask,
        operation: fn(&mut Self, PlayerId, i32, i32),
    ) {
        if mask == PlayerMask::NONE {
            return;
        }
        let (centre_x, centre_y) = self.world_to_cell(world_x, world_y);
        let cell_radius = self.world_to_cell_dist(radius).max(1);
        let circle = DiscreteCircle::new(centre_x, centre_y, cell_radius);
        self.apply_circle(&circle, mask, operation, |_, _| true);
    }

    fn apply_circle(
        &mut self,
        circle: &DiscreteCircle,
        mask: PlayerMask,
        operation: fn(&mut Self, PlayerId, i32, i32),
        keep: impl Fn(i32, i32) -> bool,
    ) {
        let (Ok(cells_x), Ok(cells_y)) = (
            i32::try_from(self.extent.cells_x),
            i32::try_from(self.extent.cells_y),
        ) else {
            return;
        };
        for index in 0..self.players {
            let player = PlayerId::new(u8::try_from(index).unwrap_or(u8::MAX));
            if !mask.holds(player) {
                continue;
            }
            for span in circle.spans() {
                if span.y < 0 || span.y >= cells_y {
                    continue;
                }
                let x_start = span.x_start.max(0);
                let x_end = span.x_end.min(cells_x.saturating_sub(1));
                let mut x = x_start;
                while x <= x_end {
                    if keep(x, span.y) {
                        operation(self, player, x, span.y);
                    }
                    let Some(next) = x.checked_add(1) else { break };
                    x = next;
                }
            }
        }
    }

    pub fn reveal_all(&mut self, player: PlayerId) {
        self.for_each_cell(player, Self::add_looker);
        self.for_each_cell(player, Self::remove_looker);
    }

    pub fn reveal_all_permanently(&mut self, player: PlayerId) {
        self.for_each_cell(player, Self::add_looker);
    }

    pub fn undo_reveal_all_permanently(&mut self, player: PlayerId) {
        self.for_each_cell(player, Self::remove_looker);
    }

    pub fn shroud_all(&mut self, player: PlayerId) {
        self.for_each_cell(player, Self::add_shrouder);
        self.for_each_cell(player, Self::remove_shrouder);
    }

    pub fn reveal_polygon_permanently(&mut self, points: &[(f32, f32)], mask: PlayerMask) {
        self.apply_polygon(points, mask, Self::add_looker);
    }

    pub fn unreveal_polygon(&mut self, points: &[(f32, f32)], mask: PlayerMask) {
        self.apply_polygon(points, mask, Self::remove_looker);
    }

    fn apply_polygon(
        &mut self,
        points: &[(f32, f32)],
        mask: PlayerMask,
        operation: fn(&mut Self, PlayerId, i32, i32),
    ) {
        if mask == PlayerMask::NONE || points.len() < 3 {
            return;
        }
        let (Ok(cells_x), Ok(cells_y)) = (
            i32::try_from(self.extent.cells_x),
            i32::try_from(self.extent.cells_y),
        ) else {
            return;
        };
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in points {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let (col_start, row_start) = self.world_to_cell(min_x, min_y);
        let (col_end, row_end) = self.world_to_cell(max_x, max_y);
        let row_start = row_start.max(0);
        let col_start = col_start.max(0);
        let row_end = row_end.min(cells_y.saturating_sub(1));
        let col_end = col_end.min(cells_x.saturating_sub(1));
        let mut row = row_start;
        while row <= row_end {
            let mut col = col_start;
            while col <= col_end {
                if point_in_polygon(self.cell_centre(col, row), points) {
                    for index in 0..self.players {
                        let player = PlayerId::new(u8::try_from(index).unwrap_or(u8::MAX));
                        if mask.holds(player) {
                            operation(self, player, col, row);
                        }
                    }
                }
                let Some(next) = col.checked_add(1) else {
                    break;
                };
                col = next;
            }
            let Some(next) = row.checked_add(1) else {
                break;
            };
            row = next;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn cell_centre(&self, col: i32, row: i32) -> (f32, f32) {
        (
            self.extent.low_x + (col as f32 + 0.5) * self.extent.cell_size,
            self.extent.low_y + (row as f32 + 0.5) * self.extent.cell_size,
        )
    }

    fn for_each_cell(&mut self, player: PlayerId, operation: fn(&mut Self, PlayerId, i32, i32)) {
        let (cells_x, cells_y) = (self.extent.cells_x, self.extent.cells_y);
        for row in 0..cells_y {
            for column in 0..cells_x {
                let (Ok(x), Ok(y)) = (i32::try_from(column), i32::try_from(row)) else {
                    continue;
                };
                operation(self, player, x, y);
            }
        }
    }

    #[must_use]
    pub fn unbalanced_cells(&self) -> usize {
        self.current
            .iter()
            .zip(self.active.iter())
            .filter(|(current, active)| **current > 1 || **active < 0)
            .count()
    }

    #[must_use]
    pub fn watched_cells(&self) -> usize {
        self.current.iter().filter(|current| **current < 0).count()
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        self.hash_extent(hasher);
        if hasher.is_legacy() {
            hasher.write_usize(self.current.len());
            for value in self.current.iter().chain(self.active.iter()) {
                hasher.write_i32(*value);
            }
        } else {
            self.current.hash_into(hasher, write_counter);
            self.active.hash_into(hasher, write_counter);
        }
        self.hash_window(hasher);
    }

    pub fn hash_into_cached(&mut self, hasher: &mut StateHasher) {
        if hasher.is_legacy() {
            self.hash_into(hasher);
            return;
        }
        self.hash_extent(hasher);
        self.current.hash_into_cached(hasher, write_counter);
        self.active.hash_into_cached(hasher, write_counter);
        self.hash_window(hasher);
    }

    fn hash_extent(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.extent.cells_x);
        hasher.write_u32(self.extent.cells_y);
        hasher.write_u32(self.players);
        hasher.write_f32(self.extent.cell_size);
        hasher.write_f32(self.extent.low_x);
        hasher.write_f32(self.extent.low_y);
    }

    fn hash_window(&self, hasher: &mut StateHasher) {
        if let Some((columns, rows)) = self.window {
            hasher.write_u32(columns);
            hasher.write_u32(rows);
        }
    }
}

fn write_counter(value: &i32, hasher: &mut StateHasher) {
    hasher.write_i32(*value);
}

fn axis_to_cell(world: f32, low: f32, cell_size: f32) -> i32 {
    if !world.is_finite() || !low.is_finite() || cell_size <= 0.0 {
        return 0;
    }
    let cell = ((world - low) / cell_size).floor();
    if cell <= -2_147_483_000.0 {
        i32::MIN.saturating_add(1_000)
    } else if cell >= 2_147_483_000.0 {
        i32::MAX.saturating_sub(1_000)
    } else {
        #[allow(clippy::cast_possible_truncation)]
        let whole = cell as i32;
        whole
    }
}

fn point_in_polygon((px, py): (f32, f32), points: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut previous = points.len().saturating_sub(1);
    for (index, &(xi, yi)) in points.iter().enumerate() {
        let Some(&(xj, yj)) = points.get(previous) else {
            break;
        };
        if (yi > py) != (yj > py) {
            let x_intersect = xi + (py - yi) / (yj - yi) * (xj - xi);
            if px < x_intersect {
                inside = !inside;
            }
        }
        previous = index;
    }
    inside
}

#[cfg(test)]
mod tests;
