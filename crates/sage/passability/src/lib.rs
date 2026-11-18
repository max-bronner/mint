#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod claims;
pub mod footprints;
pub mod water;

use mint_movement::Passability as _;
use mint_movement::{
    Cell, CellFlags, CellGeometry, CostGrid, GridExtent, HeightField, MoverClass, Vec2,
};
use mint_sage_map::{BlendTileData, HeightMapData, MAP_XY_FACTOR};

pub use footprints::{
    scaleable_ids, scaleable_walls, Footprint, FootprintRecord, Obstacle, TemplateIndex,
    WallGeometry, MAX_STAMPED_OFFSET_Z,
};
pub use water::{WaterDepths, WaterSurfaces, WetGrid, Wetness};

pub const CLIFF: CellFlags = CellFlags::bit(0);

pub const WATER: CellFlags = CellFlags::bit(1);

pub const DEEP_WATER: CellFlags = CellFlags::bit(2);

pub const OBSTACLE: CellFlags = CellFlags::bit(3);

pub const BOUNDARY: CellFlags = CellFlags::bit(4);

pub const FORD: CellFlags = CellFlags::bit(5);

pub const PINCHED: CellFlags = CellFlags::bit(6);

pub const IMPASSABLE_TO_PLAYERS: CellFlags = CellFlags::bit(7);

pub const NARROW_PASSAGE: CellFlags = CellFlags::bit(8);

pub const PORTAL: CellFlags = CellFlags::bit(9);

pub const RUBBLE: CellFlags = CellFlags::bit(10);

pub const WALL_RIM: CellFlags = CellFlags::bit(11);

pub const GATE_RESTRICTED: CellFlags = CellFlags::bit(12);

pub const RAMP_SIDE: CellFlags = CellFlags::bit(13);

pub const SCALEABLE: CellFlags = CellFlags::bit(14);

pub const DEEP_BAND: CellFlags = CellFlags::bit(15);

pub const EXTRA_PASSABLE: CellFlags = CellFlags::bit(16);

#[must_use]
pub fn halved_when_extra_passable(flags: CellFlags, cost: u32) -> u32 {
    if flags.intersects(EXTRA_PASSABLE) {
        cost / 2
    } else {
        cost
    }
}

pub const GROUND: MoverClass = MoverClass::new(
    CLIFF
        .union(WATER)
        .union(DEEP_WATER)
        .union(OBSTACLE)
        .union(RUBBLE)
        .union(BOUNDARY)
        .union(WALL_RIM)
        .union(RAMP_SIDE),
)
.line_barring(LINE_BARRED);

const LINE_BARRED: CellFlags = PINCHED.union(DEEP_BAND);

pub const GROUND_PLAYER: MoverClass =
    MoverClass::new(GROUND.blocked_by.union(IMPASSABLE_TO_PLAYERS)).line_barring(LINE_BARRED);

const SURFACE_FLAGS: CellFlags = CLIFF
    .union(WATER)
    .union(DEEP_WATER)
    .union(OBSTACLE)
    .union(RUBBLE);

#[must_use]
pub fn mover_class(surfaces: &[String], scales_walls: bool) -> Option<MoverClass> {
    let mut accepted = CellFlags::NONE;
    let mut ground = false;
    let mut air = false;
    let mut crosses_layers = false;
    let mut climbs_obstacle = false;
    for surface in surfaces {
        match surface.as_str() {
            "GROUND" => ground = true,
            "RUBBLE" => accepted = accepted.union(RUBBLE),
            "AIR" => air = true,
            "WATER" => accepted = accepted.union(WATER),
            "DEEP_WATER" => accepted = accepted.union(DEEP_WATER),
            "CLIFF" => accepted = accepted.union(CLIFF),
            "OBSTACLE" => {
                if scales_walls {
                    climbs_obstacle = true;
                } else {
                    accepted = accepted.union(OBSTACLE);
                }
            }
            "IMPASSABLE" => crosses_layers = true,
            _ => return None,
        }
    }
    if air {
        return Some(MoverClass::new(BOUNDARY));
    }
    let mut blocked = CellFlags::from_bits(SURFACE_FLAGS.bits() & !accepted.bits())
        .union(BOUNDARY)
        .union(WALL_RIM);
    if !crosses_layers {
        blocked = blocked.union(RAMP_SIDE);
    }
    let mover = MoverClass::new(blocked)
        .line_barring(LINE_BARRED)
        .object_stamped(OBSTACLE.union(RUBBLE).union(WALL_RIM));
    let mover = if climbs_obstacle {
        mover.excused(SCALEABLE)
    } else {
        mover
    };
    let mover = if crosses_layers {
        mover.crossing_layers()
    } else {
        mover
    };
    if ground {
        Some(mover)
    } else {
        Some(mover.requiring(accepted))
    }
}

pub const CLIFF_SLOPE_LIMIT: f32 = 9.8;

pub const CLIFF_SLOPE_LIMIT_RAW: f32 = 250.88;

const PAINTED_CLIFF_VERSION: u16 = 9;

fn recomputed_cliff(height_map: &HeightMapData, x: u32, y: u32) -> Option<bool> {
    corner_range(height_map, x, y).map(|range| f32::from(range) > CLIFF_SLOPE_LIMIT_RAW)
}

const PINCHED_COST: u16 = 14;

const PINCHED_FAR_COST: u32 = 10;

const NEAR_GOAL: u32 = 3;

const DEEP_BAND_STEPS: u32 = 6;

pub const ESCAPE_RINGS: u16 = 5;

#[derive(Debug)]
pub struct Passability {
    pub grid: CostGrid,
    pub geometry: CellGeometry,
    pub heights: HeightField,
    pub border: u16,
    pub playable_high: Vec2,
    boundary_cells: Vec<(u16, u16)>,
    obstacle_ids: Vec<u32>,
    rivers: Vec<bool>,
    underwater: Vec<bool>,
    surfaces: WaterSurfaces,
    pub water_depths: WaterDepths,
    counts: Counts,
}

#[derive(Debug, Clone, Copy, Default)]
struct Counts {
    cliff: u32,
    dilated: u32,
    water: u32,
    deep: u32,
    ford: u32,
    obstacle: u32,
    boundary: u32,
    pinched: u32,
    impassable_to_players: u32,
    narrow_passage: u32,
    extra_passable: u32,
    obstacles_stamped: u32,
    obstacles_off_grid: u32,
    obstacle_closed: u32,
    boundary_fallback: bool,
    blocked: u32,
    total: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Builder<'a> {
    height_map: &'a HeightMapData,
    vertical_scale: f32,
    blend_tile: Option<&'a BlendTileData>,
    water: Option<&'a mint_sage_map::Water>,
    depths: WaterDepths,
    obstacles: &'a [Obstacle],
    scaleable: &'a [u32],
    playable_border: bool,
}

impl<'a> Builder<'a> {
    #[must_use]
    pub fn new(height_map: &'a HeightMapData, vertical_scale: f32) -> Self {
        Self {
            height_map,
            vertical_scale,
            blend_tile: None,
            water: None,
            depths: WaterDepths::SHIPPED,
            obstacles: &[],
            scaleable: &[],
            playable_border: false,
        }
    }

    #[must_use]
    pub fn cliff_source(mut self, blend_tile: &'a BlendTileData) -> Self {
        self.blend_tile = Some(blend_tile);
        self
    }

    #[must_use]
    pub fn water(mut self, water: &'a mint_sage_map::Water, depths: WaterDepths) -> Self {
        self.water = Some(water);
        self.depths = depths;
        self
    }

    #[must_use]
    pub fn obstacles(mut self, obstacles: &'a [Obstacle]) -> Self {
        self.obstacles = obstacles;
        self
    }

    #[must_use]
    pub fn scaleable_obstacles(mut self, ids: &'a [u32]) -> Self {
        self.scaleable = ids;
        self
    }

    #[must_use]
    pub fn playable_border(mut self, on: bool) -> Self {
        self.playable_border = on;
        self
    }

    pub fn build(self) -> Result<Passability, String> {
        Passability::from_builder(self)
    }
}

impl Passability {
    pub fn build(height_map: &HeightMapData, vertical_scale: f32) -> Result<Self, String> {
        Builder::new(height_map, vertical_scale).build()
    }

    #[must_use]
    pub fn builder(height_map: &HeightMapData, vertical_scale: f32) -> Builder<'_> {
        Builder::new(height_map, vertical_scale)
    }

    #[must_use]
    pub fn boundary_count(&self) -> usize {
        self.boundary_cells.len()
    }

    #[must_use]
    pub fn boundary_high(&self, index: usize) -> Option<Vec2> {
        let (bx, by) = self
            .boundary_cells
            .get(index)
            .copied()
            .filter(|&(bx, by)| bx > 0 && by > 0)?;
        Some(Vec2::new(
            f32::from(bx) * MAP_XY_FACTOR,
            f32::from(by) * MAP_XY_FACTOR,
        ))
    }

    #[must_use]
    pub fn boundary_rect(&self, index: usize) -> Option<mint_movement::CellRect> {
        let (bx, by) = self
            .boundary_cells
            .get(index)
            .copied()
            .filter(|&(bx, by)| bx > 0 && by > 0)?;
        Some(mint_movement::CellRect {
            x0: self.border,
            y0: self.border,
            x1: self.border.saturating_add(bx),
            y1: self.border.saturating_add(by),
        })
    }

    fn from_builder(spec: Builder<'_>) -> Result<Self, String> {
        let height_map = spec.height_map;
        let cells_x = height_map.width.saturating_sub(1);
        let cells_y = height_map.height.saturating_sub(1);
        let width = u16::try_from(cells_x).map_err(|_| {
            format!("a height field {cells_x} cells wide is past this grid's limit")
        })?;
        let height = u16::try_from(cells_y).map_err(|_| {
            format!("a height field {cells_y} cells high is past this grid's limit")
        })?;
        let extent = GridExtent::new(width, height).map_err(|e| e.to_string())?;

        let border_cells = u16::try_from(height_map.border_size).unwrap_or(u16::MAX);
        let border = f32::from(border_cells);
        let origin = Vec2::new(-border * MAP_XY_FACTOR, -border * MAP_XY_FACTOR);
        let geometry = CellGeometry::new(MAP_XY_FACTOR, origin).map_err(|e| e.to_string())?;
        let (lo_x, hi_x, lo_y, hi_y, boundary_fallback) =
            boundary_union_rect(height_map, extent, border_cells);
        let boundary_cells: Vec<(u16, u16)> = height_map
            .boundaries
            .iter()
            .map(|b| match (u16::try_from(b.x), u16::try_from(b.y)) {
                (Ok(bx), Ok(by)) if bx > 0 && by > 0 => (bx, by),
                _ => (0, 0),
            })
            .collect();
        let playable_high = Vec2::new(
            f32::from(hi_x.saturating_sub(lo_x)) * MAP_XY_FACTOR,
            f32::from(hi_y.saturating_sub(lo_y)) * MAP_XY_FACTOR,
        );

        let scale = if spec.vertical_scale.is_finite() {
            spec.vertical_scale.abs()
        } else {
            0.0
        };
        let mut grid = CostGrid::new(extent);
        let mut counts = Counts {
            total: extent.cell_count(),
            boundary_fallback,
            ..Counts::default()
        };

        let wet = match spec.water {
            Some(water) => WetGrid::build(height_map, scale, water, spec.depths),
            None => WetGrid::dry(0, 0),
        };

        let mut extra_passable_mask =
            vec![false; usize::try_from(extent.cell_count()).unwrap_or(0)];
        let mut rivers: Vec<bool> = Vec::new();
        let mut underwater: Vec<bool> = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let cell = Cell::new(x, y);
                let (cx, cy) = (u32::from(x), u32::from(y));
                let extra_passable = spec
                    .blend_tile
                    .and_then(|bt| bt.extra_passable(cx, cy))
                    .unwrap_or(false);
                if extra_passable {
                    grid.add_flags(cell, EXTRA_PASSABLE)
                        .map_err(|e| e.to_string())?;
                    counts.extra_passable = counts.extra_passable.saturating_add(1);
                    if let Some(slot) = extra_passable_mask.get_mut(
                        usize::from(y)
                            .saturating_mul(usize::from(width))
                            .saturating_add(usize::from(x)),
                    ) {
                        *slot = true;
                    }
                }
                let painted = spec.blend_tile.and_then(|bt| {
                    if bt.version < PAINTED_CLIFF_VERSION {
                        recomputed_cliff(height_map, cx, cy).or_else(|| bt.impassable(cx, cy))
                    } else {
                        bt.impassable(cx, cy)
                    }
                });
                let cliff = !extra_passable
                    && painted.unwrap_or_else(|| is_cliff(height_map, scale, cx, cy));
                if cliff {
                    grid.add_flags(cell, CLIFF).map_err(|e| e.to_string())?;
                    counts.cliff = counts.cliff.saturating_add(1);
                }
                if let Some(true) = spec
                    .blend_tile
                    .map(|bt| bt.impassable_to_players(cx, cy).unwrap_or(false) && !extra_passable)
                {
                    grid.add_flags(cell, IMPASSABLE_TO_PLAYERS)
                        .map_err(|e| e.to_string())?;
                    counts.impassable_to_players = counts.impassable_to_players.saturating_add(1);
                }
                if let Some(true) = spec
                    .blend_tile
                    .map(|bt| bt.passage_width_narrow(cx, cy).unwrap_or(false))
                {
                    grid.add_flags(cell, NARROW_PASSAGE)
                        .map_err(|e| e.to_string())?;
                    counts.narrow_passage = counts.narrow_passage.saturating_add(1);
                }
                let wetness = last_deep_corner(&wet, cx, cy);
                if [(0, 0), (1, 0), (0, 1), (1, 1)]
                    .iter()
                    .any(|(dx, dy)| wet.in_river(cx.saturating_add(*dx), cy.saturating_add(*dy)))
                {
                    let cells = usize::try_from(extent.cell_count()).unwrap_or(0);
                    if rivers.is_empty() {
                        rivers = vec![false; cells];
                    }
                    let index = usize::from(y)
                        .saturating_mul(usize::from(width))
                        .saturating_add(usize::from(x));
                    if let Some(slot) = rivers.get_mut(index) {
                        *slot = true;
                    }
                }
                if wetness != Wetness::Dry {
                    let cells = usize::try_from(extent.cell_count()).unwrap_or(0);
                    if underwater.is_empty() {
                        underwater = vec![false; cells];
                    }
                    let index = usize::from(y)
                        .saturating_mul(usize::from(width))
                        .saturating_add(usize::from(x));
                    if let Some(slot) = underwater.get_mut(index) {
                        *slot = true;
                    }
                }
                match if cliff { Wetness::Dry } else { wetness } {
                    Wetness::Dry => {}
                    Wetness::Ford => {
                        grid.add_flags(cell, FORD).map_err(|e| e.to_string())?;
                        counts.ford = counts.ford.saturating_add(1);
                    }
                    Wetness::Water => {
                        grid.add_flags(cell, WATER).map_err(|e| e.to_string())?;
                        counts.water = counts.water.saturating_add(1);
                    }
                    Wetness::Deep => {
                        grid.add_flags(cell, DEEP_WATER)
                            .map_err(|e| e.to_string())?;
                        counts.deep = counts.deep.saturating_add(1);
                    }
                }
            }
        }

        let extra_passable_at = |cell: Cell| -> bool {
            let index = usize::from(cell.y)
                .saturating_mul(usize::from(width))
                .saturating_add(usize::from(cell.x));
            extra_passable_mask.get(index).copied().unwrap_or(false)
        };
        let first_ring = ring_around(&grid, extent, |cell| grid.flags(cell).intersects(CLIFF));
        let mut dilated = 0u32;
        for cell in &first_ring {
            if extra_passable_at(*cell) {
                continue;
            }
            grid.add_flags(*cell, CLIFF).map_err(|e| e.to_string())?;
            dilated = dilated.saturating_add(1);
        }
        counts.dilated = dilated;

        deep_band(&mut grid, extent)?;

        let mut obstacle_ids = Vec::new();
        if !spec.obstacles.is_empty() {
            let cells = usize::try_from(extent.cell_count()).unwrap_or(0);
            obstacle_ids = vec![u32::MAX; cells];
            stamp_obstacles(
                &mut grid,
                &mut obstacle_ids,
                extent,
                geometry,
                spec.obstacles,
                spec.scaleable,
                &mut counts,
            )?;
            if let Some(bounds) = obstacle_cell_bounds(&obstacle_ids, extent) {
                apply_obstacle_border_passes(
                    &mut grid,
                    extent,
                    grow_bounds(bounds, extent, 2),
                    &mut counts,
                )?;
            }
        }

        if spec.playable_border {
            counts.boundary = mark_border(&mut grid, extent, lo_x, hi_x, lo_y, hi_y)?;
        }

        for y in 0..height {
            for x in 0..width {
                let cell = Cell::new(x, y);
                let flags = grid.flags(cell);
                if flags.intersects(GROUND.blocked_by) {
                    counts.blocked = counts.blocked.saturating_add(1);
                }
                let mut cost = 0u16;
                if flags.intersects(PINCHED) {
                    cost = cost.saturating_add(PINCHED_COST);
                }
                if cost != 0 {
                    grid.set_extra_cost(cell, cost).map_err(|e| e.to_string())?;
                }
            }
        }

        Ok(Self {
            grid,
            geometry,
            heights: height_field(height_map, spec.vertical_scale)?,
            border: border_cells,
            playable_high,
            boundary_cells,
            obstacle_ids,
            rivers,
            underwater,
            surfaces: match spec.water {
                Some(water) => WaterSurfaces::build(water),
                None => WaterSurfaces::default(),
            },
            water_depths: spec.depths,
            counts,
        })
    }

    #[must_use]
    pub fn in_river(&self, point: Vec2) -> bool {
        let Some(cell) = self.cell_at(point) else {
            return false;
        };
        let index = usize::from(cell.y)
            .saturating_mul(usize::from(self.grid.extent().width))
            .saturating_add(usize::from(cell.x));
        self.rivers.get(index).copied().unwrap_or(false)
    }

    #[must_use]
    pub fn underwater(&self, point: Vec2) -> bool {
        let Some(cell) = self.cell_at(point) else {
            return false;
        };
        let index = usize::from(cell.y)
            .saturating_mul(usize::from(self.grid.extent().width))
            .saturating_add(usize::from(cell.x));
        self.underwater.get(index).copied().unwrap_or(false)
    }

    #[must_use]
    pub fn has_water(&self) -> bool {
        !self.surfaces.is_empty()
    }

    #[must_use]
    pub fn water_surface_at(&self, point: Vec2) -> Option<f32> {
        self.surfaces.surface_at([point.x, point.y])
    }

    #[must_use]
    pub fn water_depth(&self, point: Vec2) -> f32 {
        let (Some(surface), Some(ground)) = (
            self.water_surface_at(point),
            self.heights.triangle_height_at(point.x, point.y),
        ) else {
            return 0.0;
        };
        (surface - ground).max(0.0)
    }

    #[must_use]
    pub fn ground_range(&self) -> (f32, f32) {
        let extent = self.heights.extent();
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        for y in 0..extent.height {
            for x in 0..extent.width {
                if let Some(sample) = self.heights.sample(x, y) {
                    low = low.min(sample);
                    high = high.max(sample);
                }
            }
        }
        if low > high {
            (0.0, 0.0)
        } else {
            (low, high)
        }
    }

    #[must_use]
    pub fn blocked_cells(&self) -> u32 {
        self.counts.blocked
    }

    #[must_use]
    pub fn costly_cells(&self) -> u32 {
        self.counts.pinched
    }

    #[must_use]
    pub fn obstacle_closed_cells(&self) -> u32 {
        self.counts.obstacle_closed
    }

    #[must_use]
    pub fn pinched_cells(&self) -> u32 {
        self.counts.pinched
    }

    #[must_use]
    pub fn report(&self) -> String {
        let c = self.counts;
        let blocked = c.blocked;
        let (low, high) = self.ground_range();
        format!(
            "passability: {}x{} cells, ground {:.0}..{:.0}, {blocked} blocked ({:.2}%) = {} cliff + {} dilated + \
             {} water + {} deep + {} obstacle + {} boundary{}; {} pinched, {} ford, {} narrow, {} obstacle-closed; \
             {} obstacle(s) stamped, {} off the grid",
            self.grid.extent().width,
            self.grid.extent().height,
            low,
            high,
            100.0 * f64::from(blocked) / f64::from(c.total.max(1)),
            c.cliff,
            c.dilated,
            c.water,
            c.deep,
            c.obstacle,
            c.boundary,
            if c.boundary_fallback {
                " (no boundary rect, ring fallback)"
            } else {
                ""
            },
            c.pinched,
            c.ford,
            c.narrow_passage,
            c.obstacle_closed,
            c.obstacles_stamped,
            c.obstacles_off_grid,
        )
    }

    pub fn cell_at(&self, point: Vec2) -> Option<Cell> {
        self.geometry.cell_at(point, 0, self.grid.extent())
    }

    #[must_use]
    pub fn obstacle_at(&self, cell: Cell) -> Option<u32> {
        let index = usize::try_from(self.grid.extent().index(cell)?).ok()?;
        match self.obstacle_ids.get(index).copied() {
            Some(u32::MAX) | None => None,
            Some(id) => Some(id),
        }
    }

    #[must_use]
    pub fn nearest_passable(&self, cell: Cell, mover: MoverClass) -> Option<Cell> {
        nearest_passable_over(&self.grid, cell, mover)
    }

    #[must_use]
    pub fn covering_rect(&self, centre: Vec2, reach: f32) -> Option<CellRect> {
        if !reach.is_finite() || reach <= 0.0 {
            return None;
        }
        let low = Vec2::new(centre.x - reach, centre.y - reach);
        let high = Vec2::new(centre.x + reach, centre.y + reach);
        cell_rect(self.geometry, self.grid.extent(), low, high)
    }

    #[must_use]
    pub fn nearest_passable_around(&self, rect: CellRect, mover: MoverClass) -> Option<Cell> {
        nearest_passable_around_over(&self.grid, rect, mover)
    }
}

#[must_use]
pub fn nearest_passable_over(
    passable: &dyn mint_movement::Passability,
    cell: Cell,
    mover: MoverClass,
) -> Option<Cell> {
    if passable.is_passable(cell, mover) {
        return Some(cell);
    }
    let extent = passable.extent();
    for ring in 1..=ESCAPE_RINGS {
        let ring = i32::from(ring);
        let mut best: Option<Cell> = None;
        let from = ring.saturating_neg();
        for dy in from..=ring {
            for dx in from..=ring {
                if dx.abs() != ring && dy.abs() != ring {
                    continue;
                }
                let (Ok(x), Ok(y)) = (
                    u16::try_from(i32::from(cell.x).saturating_add(dx)),
                    u16::try_from(i32::from(cell.y).saturating_add(dy)),
                ) else {
                    continue;
                };
                let candidate = Cell::on_layer(x, y, cell.layer);
                if !extent.contains(candidate) || !passable.is_passable(candidate, mover) {
                    continue;
                }
                if best.is_none() {
                    best = Some(candidate);
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

#[must_use]
pub fn nearest_passable_around_over(
    passable: &dyn mint_movement::Passability,
    rect: CellRect,
    mover: MoverClass,
) -> Option<Cell> {
    let extent = passable.extent();
    for ring in 0..=ESCAPE_RINGS {
        let ring = i32::from(ring);
        let x0 = i32::from(rect.x0).saturating_sub(ring);
        let x1 = i32::from(rect.x1).saturating_add(ring);
        let y0 = i32::from(rect.y0).saturating_sub(ring);
        let y1 = i32::from(rect.y1).saturating_add(ring);
        let mut best: Option<Cell> = None;
        for y in y0..=y1 {
            for x in x0..=x1 {
                if x != x0 && x != x1 && y != y0 && y != y1 {
                    continue;
                }
                let (Ok(cx), Ok(cy)) = (u16::try_from(x), u16::try_from(y)) else {
                    continue;
                };
                let candidate = Cell::new(cx, cy);
                if !extent.contains(candidate) || !passable.is_passable(candidate, mover) {
                    continue;
                }
                if best.is_none() {
                    best = Some(candidate);
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

const POSITION_RING_SPACING: f32 = 5.0;

#[must_use]
pub fn find_position_around(
    passable: &dyn mint_movement::Passability,
    geometry: CellGeometry,
    centre: Vec2,
    (min_radius, max_radius, start_angle): (f32, f32, f32),
    mover: MoverClass,
    accept: impl Fn(Vec2) -> bool,
) -> Option<Vec2> {
    let extent = passable.extent();
    let fits = |angle: f32, distance: f32| {
        let (sin, cos) = mint_math::f32::sin_cos(angle);
        let at = Vec2::new(centre.x + distance * cos, centre.y + distance * sin);
        geometry
            .cell_at(at, 0, extent)
            .filter(|cell| passable.is_passable(*cell, mover))
            .filter(|_| accept(at))
            .map(|_| at)
    };
    if !min_radius.is_finite() || !max_radius.is_finite() {
        return None;
    }
    let mut distance = min_radius.max(0.0);
    let mut first_ring = true;
    while distance <= max_radius {
        let spacing = if first_ring {
            core::f32::consts::TAU
        } else {
            (POSITION_RING_SPACING / (distance + 1.0)) * (core::f32::consts::TAU / 6.0)
        };
        let mut offset = 0.0_f32;
        while offset < core::f32::consts::PI {
            let found = fits(start_angle + offset, distance).or_else(|| {
                (offset > 0.0)
                    .then(|| fits(start_angle - offset, distance))
                    .flatten()
            });
            if found.is_some() {
                return found;
            }
            offset += spacing;
        }
        distance += POSITION_RING_SPACING;
        first_ring = false;
    }
    None
}

#[derive(Debug, Clone, Copy)]
pub struct DynamicView<'a> {
    pub grid: &'a CostGrid,
    pub counts: &'a [u8],
    pub rubble: &'a [u8],
    pub wall_rim: &'a [u8],
    pub tags: &'a [u8],
    pub connect: &'a [u8],
    pub released: &'a [u8],
    pub closed: &'a [u8],
    pub obstacle_pinched: &'a [u8],
    pub ramp_side: &'a [u8],
    pub scaleable: &'a [u8],
    pub barred: &'a [u8],
}

const GROUND_TAG: u8 = 1;

const RAMP_TAG: u8 = 16;

const FIRST_WALL_TAG: u8 = 17;

const WALL_TOP_CLEARS: CellFlags = CellFlags::from_bits(
    CLIFF.bits()
        | WATER.bits()
        | DEEP_WATER.bits()
        | FORD.bits()
        | OBSTACLE.bits()
        | PINCHED.bits(),
);

impl DynamicView<'_> {
    fn tag_at(&self, cell: Cell) -> u8 {
        let index = self
            .extent()
            .index(cell)
            .and_then(|index| usize::try_from(index).ok());
        index
            .and_then(|index| self.tags.get(index))
            .copied()
            .unwrap_or(GROUND_TAG)
    }

    fn connect_at(&self, cell: Cell) -> u8 {
        let index = self
            .extent()
            .index(cell)
            .and_then(|index| usize::try_from(index).ok());
        index
            .and_then(|index| self.connect.get(index))
            .copied()
            .unwrap_or(0)
    }
}

impl mint_movement::Passability for DynamicView<'_> {
    fn extent(&self) -> GridExtent {
        self.grid.extent()
    }

    fn flags(&self, cell: Cell) -> CellFlags {
        let index = self
            .extent()
            .index(cell)
            .and_then(|index| usize::try_from(index).ok());
        let held = |counts: &[u8]| {
            index
                .and_then(|index| counts.get(index))
                .is_some_and(|count| *count > 0)
        };
        let tag = self.tag_at(cell);
        let on_wall_or_ramp =
            tag == RAMP_TAG || self.connect_at(cell) == RAMP_TAG || tag >= FIRST_WALL_TAG;
        let clears = if on_wall_or_ramp {
            WALL_TOP_CLEARS
        } else {
            CellFlags::NONE
        };
        let grid_flags = self.grid.flags(cell);
        let mut flags = CellFlags::from_bits(grid_flags.bits() & !clears.bits());
        if held(self.counts) && !(clears.intersects(OBSTACLE) && held(self.released)) {
            flags = flags.union(OBSTACLE);
        }
        if held(self.rubble) {
            flags = flags.union(RUBBLE);
        }
        if held(self.wall_rim) {
            flags = flags.union(WALL_RIM);
        }
        if held(self.closed) && !(clears.intersects(OBSTACLE) && held(self.released)) {
            flags = flags.union(OBSTACLE);
        }
        if held(self.obstacle_pinched) && !clears.intersects(PINCHED) {
            flags = flags.union(PINCHED);
        }
        if on_wall_or_ramp {
            flags = flags.union(GATE_RESTRICTED);
        }
        if held(self.ramp_side) {
            flags = flags.union(RAMP_SIDE);
        }
        if held(self.scaleable) {
            flags = flags.union(SCALEABLE);
        }
        match index.and_then(|index| self.barred.get(index)).copied() {
            Some(1) => flags = flags.union(IMPASSABLE_TO_PLAYERS),
            Some(2) => flags = CellFlags::from_bits(flags.bits() & !IMPASSABLE_TO_PLAYERS.bits()),
            _ => {}
        }
        flags
    }

    fn extra_cost(&self, cell: Cell) -> u32 {
        self.grid.extra_cost(cell)
    }

    fn goal_extra_cost(&self, cell: Cell, goal: Option<Cell>) -> u32 {
        let flags = self.flags(cell);
        if !flags.intersects(PINCHED.union(DEEP_BAND)) {
            return 0;
        }
        let near = goal.is_some_and(|goal| {
            u32::from(cell.x.abs_diff(goal.x)).saturating_add(u32::from(cell.y.abs_diff(goal.y)))
                <= NEAR_GOAL
        });
        if near {
            0
        } else {
            halved_when_extra_passable(flags, PINCHED_FAR_COST)
        }
    }

    fn tail_clear(&self, cell: Cell, _mover: mint_movement::MoverClass) -> bool {
        self.tag_at(cell) == GROUND_TAG
    }

    fn admits_step(&self, from: Cell, to: Cell, mover: mint_movement::MoverClass) -> bool {
        if mover.crosses_layers {
            return true;
        }
        let (from_tag, from_connect) = (self.tag_at(from), self.connect_at(from));
        let (to_tag, to_connect) = (self.tag_at(to), self.connect_at(to));
        to_tag == from_tag
            || to_tag == from_connect
            || to_connect == from_tag
            || (from_connect == RAMP_TAG && to_connect == from_connect)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GatedView<'a> {
    pub ground: DynamicView<'a>,
    pub links: &'a [mint_movement::PortalLink],
}

impl mint_movement::Passability for GatedView<'_> {
    fn extent(&self) -> GridExtent {
        self.ground.extent()
    }

    fn flags(&self, cell: Cell) -> CellFlags {
        self.ground.flags(cell)
    }

    fn extra_cost(&self, cell: Cell) -> u32 {
        self.ground.extra_cost(cell)
    }

    fn goal_extra_cost(&self, cell: Cell, goal: Option<Cell>) -> u32 {
        self.ground.goal_extra_cost(cell, goal)
    }

    fn tail_clear(&self, cell: Cell, mover: mint_movement::MoverClass) -> bool {
        self.ground.tail_clear(cell, mover)
    }

    fn admits_step(&self, from: Cell, to: Cell, mover: mint_movement::MoverClass) -> bool {
        self.ground.admits_step(from, to, mover)
    }

    fn portal_links(&self, cell: Cell) -> &[mint_movement::PortalLink] {
        let first = self.links.partition_point(|link| link.entry < cell);
        let count = self.links.get(first..).map_or(0, |rest| {
            rest.iter().take_while(|link| link.entry == cell).count()
        });
        self.links
            .get(first..first.saturating_add(count))
            .unwrap_or(&[])
    }
}

pub fn height_field(
    height_map: &HeightMapData,
    vertical_scale: f32,
) -> Result<HeightField, String> {
    let width = u16::try_from(height_map.width).map_err(|_| {
        format!(
            "a heightmap {} samples wide is past the limit",
            height_map.width
        )
    })?;
    let height = u16::try_from(height_map.height).map_err(|_| {
        format!(
            "a heightmap {} samples high is past the limit",
            height_map.height
        )
    })?;
    let extent = GridExtent::new(width, height).map_err(|e| e.to_string())?;
    let border = f32::from(u16::try_from(height_map.border_size).unwrap_or(u16::MAX));
    let origin = Vec2::new(-border * MAP_XY_FACTOR, -border * MAP_XY_FACTOR);
    let geometry = CellGeometry::new(MAP_XY_FACTOR, origin).map_err(|e| e.to_string())?;
    let scale = if vertical_scale.is_finite() {
        vertical_scale
    } else {
        0.0
    };
    let samples: Vec<f32> = (0..extent.cell_count())
        .map(|index| {
            height_map
                .samples
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .map_or(0.0, |sample| f32::from(*sample) * scale)
        })
        .collect();
    HeightField::new(extent, geometry, samples).map_err(|e| e.to_string())
}

#[must_use]
pub fn is_cliff(height_map: &HeightMapData, vertical_scale: f32, x: u32, y: u32) -> bool {
    let scale = if vertical_scale.is_finite() {
        vertical_scale.abs()
    } else {
        0.0
    };
    corner_range(height_map, x, y).is_some_and(|range| f32::from(range) * scale > CLIFF_SLOPE_LIMIT)
}

fn corner_range(height_map: &HeightMapData, x: u32, y: u32) -> Option<u16> {
    let a = height_map.sample(x, y)?;
    let b = height_map.sample(x.saturating_add(1), y)?;
    let c = height_map.sample(x.saturating_add(1), y.saturating_add(1))?;
    let d = height_map.sample(x, y.saturating_add(1))?;
    let low = a.min(b).min(c).min(d);
    let high = a.max(b).max(c).max(d);
    Some(high.saturating_sub(low))
}

fn last_deep_corner(wet: &WetGrid, x: u32, y: u32) -> Wetness {
    let (nx, ny) = (x.saturating_add(1), y.saturating_add(1));
    let mut result = Wetness::Dry;
    for (cx, cy) in [(x, y), (x, ny), (nx, ny), (nx, y)] {
        match wet.at(cx, cy) {
            Wetness::Dry => {}
            Wetness::Ford => {
                if result == Wetness::Dry {
                    result = Wetness::Ford;
                }
            }
            deeper => result = deeper,
        }
    }
    result
}

fn deep_band(grid: &mut CostGrid, extent: GridExtent) -> Result<(), String> {
    const ORTHOGONAL: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    let neighbours = |cell: Cell| {
        ORTHOGONAL.iter().filter_map(move |(dx, dy)| {
            let nx = u16::try_from(i32::from(cell.x).saturating_add(*dx)).ok()?;
            let ny = u16::try_from(i32::from(cell.y).saturating_add(*dy)).ok()?;
            let neighbour = Cell::new(nx, ny);
            extent.contains(neighbour).then_some(neighbour)
        })
    };
    let water = WATER.union(DEEP_WATER);
    let mut band = Vec::new();
    for y in 0..extent.height {
        for x in 0..extent.width {
            let cell = Cell::new(x, y);
            if grid.flags(cell).intersects(DEEP_WATER)
                && neighbours(cell).any(|n| !grid.flags(n).intersects(DEEP_WATER))
            {
                band.push(cell);
            }
        }
    }
    for cell in &band {
        grid.add_flags(*cell, DEEP_BAND)
            .map_err(|e| e.to_string())?;
    }
    for _ in 0..DEEP_BAND_STEPS {
        let mut grown = Vec::new();
        for y in 0..extent.height {
            for x in 0..extent.width {
                let cell = Cell::new(x, y);
                if grid.flags(cell).intersects(water)
                    && !grid.flags(cell).intersects(DEEP_BAND)
                    && neighbours(cell).any(|n| grid.flags(n).intersects(DEEP_BAND))
                {
                    grown.push(cell);
                }
            }
        }
        for cell in &grown {
            grid.add_flags(*cell, DEEP_BAND)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn ring_around(grid: &CostGrid, extent: GridExtent, marked: impl Fn(Cell) -> bool) -> Vec<Cell> {
    let mut ring = Vec::new();
    for y in 0..extent.height {
        for x in 0..extent.width {
            let cell = Cell::new(x, y);
            if grid
                .flags(cell)
                .intersects(CLIFF.union(WATER).union(DEEP_WATER).union(OBSTACLE))
            {
                continue;
            }
            let touches = (-1i32..=1).any(|dy| {
                (-1i32..=1).any(|dx| {
                    if dx == 0 && dy == 0 {
                        return false;
                    }
                    let (nx, ny) = (
                        i32::from(x).saturating_add(dx),
                        i32::from(y).saturating_add(dy),
                    );
                    u16::try_from(nx)
                        .ok()
                        .zip(u16::try_from(ny).ok())
                        .map(|(nx, ny)| Cell::new(nx, ny))
                        .filter(|neighbour| extent.contains(*neighbour))
                        .is_some_and(&marked)
                })
            });
            if touches {
                ring.push(cell);
            }
        }
    }
    ring
}

fn stamp_obstacles(
    grid: &mut CostGrid,
    ids: &mut [u32],
    extent: GridExtent,
    geometry: CellGeometry,
    obstacles: &[Obstacle],
    scaleable: &[u32],
    counts: &mut Counts,
) -> Result<(), String> {
    for obstacle in obstacles {
        let cells = covered_cells(geometry, extent, obstacle);
        if cells.is_empty() {
            counts.obstacles_off_grid = counts.obstacles_off_grid.saturating_add(1);
            continue;
        }
        let scaleable_wall = scaleable.contains(&obstacle.id);
        for cell in cells {
            if !grid.flags(cell).intersects(OBSTACLE) {
                counts.obstacle = counts.obstacle.saturating_add(1);
            }
            grid.add_flags(cell, OBSTACLE).map_err(|e| e.to_string())?;
            if scaleable_wall {
                grid.add_flags(cell, SCALEABLE).map_err(|e| e.to_string())?;
            }
            if let Some(slot) = extent
                .index(cell)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| ids.get_mut(index))
            {
                *slot = obstacle.id;
            }
        }
        counts.obstacles_stamped = counts.obstacles_stamped.saturating_add(1);
    }
    Ok(())
}

fn obstacle_cell_bounds(obstacle_ids: &[u32], extent: GridExtent) -> Option<CellRect> {
    let mut bounds: Option<CellRect> = None;
    for (index, id) in obstacle_ids.iter().enumerate() {
        if *id == u32::MAX {
            continue;
        }
        let Some(index) = u32::try_from(index).ok() else {
            continue;
        };
        let Some(cell) = extent.cell_at_index(index) else {
            continue;
        };
        bounds = Some(match bounds {
            None => CellRect {
                x0: cell.x,
                x1: cell.x,
                y0: cell.y,
                y1: cell.y,
            },
            Some(r) => CellRect {
                x0: r.x0.min(cell.x),
                x1: r.x1.max(cell.x),
                y0: r.y0.min(cell.y),
                y1: r.y1.max(cell.y),
            },
        });
    }
    bounds
}

#[must_use]
pub fn grow_bounds(bounds: CellRect, extent: GridExtent, margin: u16) -> CellRect {
    CellRect {
        x0: bounds.x0.saturating_sub(margin),
        x1: bounds
            .x1
            .saturating_add(margin)
            .min(extent.width.saturating_sub(1)),
        y0: bounds.y0.saturating_sub(margin),
        y1: bounds
            .y1
            .saturating_add(margin)
            .min(extent.height.saturating_sub(1)),
    }
}

fn offset_cell(cell: Cell, dx: i32, dy: i32, extent: GridExtent) -> Option<Cell> {
    let nx = i32::from(cell.x).checked_add(dx)?;
    let ny = i32::from(cell.y).checked_add(dy)?;
    let candidate = Cell::new(u16::try_from(nx).ok()?, u16::try_from(ny).ok()?);
    extent.contains(candidate).then_some(candidate)
}

#[must_use]
pub fn obstacle_border_passes(
    bounds: CellRect,
    extent: GridExtent,
    is_clear: impl Fn(Cell) -> bool,
    is_obstacle: impl Fn(Cell) -> bool,
    is_ground_layer: impl Fn(Cell) -> bool,
) -> (Vec<Cell>, Vec<Cell>) {
    let bw = usize::from(bounds.x1.saturating_sub(bounds.x0).saturating_add(1));
    let bh = usize::from(bounds.y1.saturating_sub(bounds.y0).saturating_add(1));
    let mut closed_mask = vec![false; bw.saturating_mul(bh)];
    let local = |cell: Cell| -> Option<usize> {
        if cell.x < bounds.x0 || cell.x > bounds.x1 || cell.y < bounds.y0 || cell.y > bounds.y1 {
            return None;
        }
        Some(
            usize::from(cell.y.saturating_sub(bounds.y0))
                .saturating_mul(bw)
                .saturating_add(usize::from(cell.x.saturating_sub(bounds.x0))),
        )
    };
    let mut closed = Vec::new();
    for y in bounds.y0..=bounds.y1 {
        for x in bounds.x0..=bounds.x1 {
            let cell = Cell::new(x, y);
            if !is_clear(cell) {
                continue;
            }
            let (mut orth, mut total) = (0u8, 0u8);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let Some(neighbour) = offset_cell(cell, dx, dy, extent) else {
                        continue;
                    };
                    if is_clear(neighbour) {
                        total = total.saturating_add(1);
                        if dx == 0 || dy == 0 {
                            orth = orth.saturating_add(1);
                        }
                    }
                }
            }
            if orth < 2 || total < 4 {
                closed.push(cell);
                if let Some(index) = local(cell) {
                    if let Some(slot) = closed_mask.get_mut(index) {
                        *slot = true;
                    }
                }
            }
        }
    }
    let is_closed =
        |cell: Cell| local(cell).and_then(|i| closed_mask.get(i)).copied() == Some(true);
    let mut pinched = Vec::new();
    for y in bounds.y0..=bounds.y1 {
        for x in bounds.x0..=bounds.x1 {
            let cell = Cell::new(x, y);
            if is_closed(cell) || !is_clear(cell) || !is_ground_layer(cell) {
                continue;
            }
            let touches = [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)]
                .iter()
                .any(|(dx, dy)| {
                    offset_cell(cell, *dx, *dy, extent)
                        .is_some_and(|neighbour| is_obstacle(neighbour) || is_closed(neighbour))
                });
            if touches {
                pinched.push(cell);
            }
        }
    }
    (closed, pinched)
}

fn apply_obstacle_border_passes(
    grid: &mut CostGrid,
    extent: GridExtent,
    bounds: CellRect,
    counts: &mut Counts,
) -> Result<(), String> {
    let is_clear = |cell: Cell| !grid.flags(cell).intersects(GROUND.blocked_by);
    let is_obstacle = |cell: Cell| grid.flags(cell).intersects(OBSTACLE);
    let (closed, pinched) = obstacle_border_passes(bounds, extent, is_clear, is_obstacle, |_| true);
    for cell in &closed {
        grid.add_flags(*cell, OBSTACLE).map_err(|e| e.to_string())?;
        grid.remove_flags(*cell, PINCHED)
            .map_err(|e| e.to_string())?;
    }
    counts.obstacle_closed = u32::try_from(closed.len()).unwrap_or(u32::MAX);
    for cell in &pinched {
        grid.add_flags(*cell, PINCHED).map_err(|e| e.to_string())?;
    }
    counts.pinched = counts
        .pinched
        .saturating_add(u32::try_from(pinched.len()).unwrap_or(u32::MAX));
    Ok(())
}

#[must_use]
pub fn covered_cells(geometry: CellGeometry, extent: GridExtent, obstacle: &Obstacle) -> Vec<Cell> {
    let reach = obstacle.reach();
    if !reach.is_finite() || reach <= 0.0 {
        return Vec::new();
    }
    let cell_size = geometry.cell_size();
    let margin = 0.4 * cell_size;
    let low = Vec2::new(
        obstacle.centre.x - reach - margin,
        obstacle.centre.y - reach - margin,
    );
    let high = Vec2::new(
        obstacle.centre.x + reach + margin,
        obstacle.centre.y + reach + margin,
    );
    let Some(rect) = cell_rect(geometry, extent, low, high) else {
        return Vec::new();
    };
    let (sin, cos) = mint_math::f32::sin_cos(obstacle.angle);
    let mut cells = Vec::new();
    for record in &obstacle.records {
        let record_centre = Vec2::new(
            obstacle.centre.x + record.offset.x * cos - record.offset.y * sin,
            obstacle.centre.y + record.offset.x * sin + record.offset.y * cos,
        );
        match record.footprint {
            Footprint::Disc { radius } => {
                disc_cells(geometry, rect, record_centre, radius, cell_size, &mut cells);
            }
            Footprint::Box { major, minor } => {
                box_cells(
                    geometry,
                    rect,
                    record_centre,
                    major,
                    minor,
                    sin,
                    cos,
                    cell_size,
                    &mut cells,
                );
            }
        }
    }
    cells.sort_unstable_by_key(|cell| (cell.x, cell.y));
    cells.dedup();
    cells
}

fn disc_cells(
    geometry: CellGeometry,
    rect: CellRect,
    centre: Vec2,
    radius: f32,
    cell_size: f32,
    out: &mut Vec<Cell>,
) {
    let inflated = radius + 0.4 * cell_size;
    if !inflated.is_finite() {
        return;
    }
    let inflated_sq = inflated * inflated;
    for y in rect.y0..=rect.y1 {
        for x in rect.x0..=rect.x1 {
            let cell = Cell::new(x, y);
            let point = geometry.centre_of(cell);
            let dx = point.x - centre.x;
            let dy = point.y - centre.y;
            if dx * dx + dy * dy <= inflated_sq {
                out.push(cell);
            }
        }
    }
}

const BOX_SAMPLE_SLACK: u32 = 4;

#[allow(clippy::too_many_arguments)]
fn box_cells(
    geometry: CellGeometry,
    rect: CellRect,
    centre: Vec2,
    major: f32,
    minor: f32,
    sin: f32,
    cos: f32,
    cell_size: f32,
    out: &mut Vec<Cell>,
) {
    if !major.is_finite() || !minor.is_finite() || major <= 0.0 || minor <= 0.0 {
        return;
    }
    let step = cell_size * 0.5;
    if !step.is_finite() || step <= 0.0 {
        return;
    }
    let cap_x = u32::from(rect.x1.saturating_sub(rect.x0))
        .saturating_add(1)
        .saturating_mul(2)
        .saturating_add(BOX_SAMPLE_SLACK);
    let cap_y = u32::from(rect.y1.saturating_sub(rect.y0))
        .saturating_add(1)
        .saturating_mul(2)
        .saturating_add(BOX_SAMPLE_SLACK);
    let steps = |halfsize: f32, cap: u32| -> u32 {
        let raw = (2.0 * halfsize / step).ceil();
        if !raw.is_finite() || raw <= 0.0 {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let raw = raw as u32;
        raw.min(cap)
    };
    let steps_x = steps(major, cap_x);
    let steps_y = steps(minor, cap_y);
    let ydx = sin * step;
    let ydy = -cos * step;
    let xdx = cos * step;
    let xdy = sin * step;
    let origin = geometry.origin();
    let mut row_x = centre.x - major * cos - minor * sin;
    let mut row_y = centre.y + minor * cos - major * sin;
    for _ in 0..steps_y {
        let mut x = row_x;
        let mut y = row_y;
        for _ in 0..steps_x {
            let cx = ((x - origin.x + 0.5) / cell_size).floor();
            let cy = ((y - origin.y + 0.5) / cell_size).floor();
            if cx.is_finite() && cy.is_finite() && cx >= 0.0 && cy >= 0.0 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (cx, cy) = (cx as u32, cy as u32);
                if let (Ok(cx), Ok(cy)) = (u16::try_from(cx), u16::try_from(cy)) {
                    if cx >= rect.x0 && cx <= rect.x1 && cy >= rect.y0 && cy <= rect.y1 {
                        out.push(Cell::new(cx, cy));
                    }
                }
            }
            x += xdx;
            y += xdy;
        }
        row_x += ydx;
        row_y += ydy;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRect {
    pub x0: u16,
    pub x1: u16,
    pub y0: u16,
    pub y1: u16,
}

fn cell_rect(
    geometry: CellGeometry,
    extent: GridExtent,
    low: Vec2,
    high: Vec2,
) -> Option<CellRect> {
    if !low.x.is_finite() || !low.y.is_finite() || !high.x.is_finite() || !high.y.is_finite() {
        return None;
    }
    let clamp = |world: f32, origin: f32, limit: u16| -> Option<u16> {
        let raw = ((world - origin) / geometry.cell_size()).floor();
        if !raw.is_finite() {
            return None;
        }
        if raw < 0.0 {
            return Some(0);
        }
        if raw >= f32::from(limit) {
            return limit.checked_sub(1);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(raw as u16)
    };
    let origin = geometry.centre_of(Cell::new(0, 0));
    let half = geometry.cell_size() * 0.5;
    let (ox, oy) = (origin.x - half, origin.y - half);
    let far_x = ox + f32::from(extent.width) * geometry.cell_size();
    let far_y = oy + f32::from(extent.height) * geometry.cell_size();
    if high.x < ox || high.y < oy || low.x >= far_x || low.y >= far_y {
        return None;
    }
    Some(CellRect {
        x0: clamp(low.x, ox, extent.width)?,
        x1: clamp(high.x, ox, extent.width)?,
        y0: clamp(low.y, oy, extent.height)?,
        y1: clamp(high.y, oy, extent.height)?,
    })
}

fn boundary_union_rect(
    height_map: &HeightMapData,
    extent: GridExtent,
    border: u16,
) -> (u16, u16, u16, u16, bool) {
    let boundary = height_map
        .boundaries
        .iter()
        .filter_map(|b| Some((u16::try_from(b.x).ok()?, u16::try_from(b.y).ok()?)))
        .filter(|&(bx, by)| bx > 0 && by > 0)
        .fold(None, |union: Option<(u16, u16)>, (bx, by)| {
            Some(union.map_or((bx, by), |(ux, uy)| (ux.max(bx), uy.max(by))))
        });
    match boundary {
        Some((bx, by)) => (
            border,
            border.saturating_add(bx),
            border,
            border.saturating_add(by),
            false,
        ),
        None => (
            border,
            extent.width.saturating_sub(border),
            border,
            extent.height.saturating_sub(border),
            true,
        ),
    }
}

fn mark_border(
    grid: &mut CostGrid,
    extent: GridExtent,
    lo_x: u16,
    hi_x: u16,
    lo_y: u16,
    hi_y: u16,
) -> Result<u32, String> {
    if hi_x <= lo_x || hi_y <= lo_y {
        return Ok(0);
    }
    let mut marked = 0u32;
    for y in 0..extent.height {
        for x in 0..extent.width {
            if x >= lo_x && x < hi_x && y >= lo_y && y < hi_y {
                continue;
            }
            let cell = Cell::new(x, y);
            if !grid.flags(cell).intersects(BOUNDARY) {
                marked = marked.saturating_add(1);
            }
            grid.add_flags(cell, BOUNDARY).map_err(|e| e.to_string())?;
        }
    }
    Ok(marked)
}

#[cfg(test)]
mod tests;
