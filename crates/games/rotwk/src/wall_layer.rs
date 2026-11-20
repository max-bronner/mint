use crate::diag::diag;
use mint_movement::{Cell, CellGeometry, GridExtent, HeightField, Vec2};
use mint_sim::{Component, ComponentId, Entity, StateHasher, Tick, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Layer(pub(crate) u8);

impl Layer {
    pub(crate) const GROUND: Layer = Layer(1);
    pub(crate) const RAMPS: Layer = Layer(16);
    const FIRST_WALL: u8 = FIRST_WALL_TAG;

    #[must_use]
    pub(crate) fn is_wall_top(self) -> bool {
        self.0 >= Self::FIRST_WALL
    }
}

impl Component for Layer {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.0);
    }
}

const GROUND_TAG: u8 = 1;

pub(crate) const FIRST_WALL_TAG: u8 = 17;

const MAX_WALL_TAGS: usize = 64;

const HEIGHT_MERGE_TOLERANCE: f32 = 3.5;

const RAMP_END_TOLERANCE: f32 = 15.0;

const STICKY_LAYER_TOLERANCE: f32 = 10.0;

const RAMP_TAG: u8 = Layer::RAMPS.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Polygon {
    pub(crate) perimeter: Vec<Vec2>,
    pub(crate) height: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct Stamp {
    entity: Entity,
    polygon: Polygon,
    released: Vec<Cell>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ramp {
    pub(crate) from_left: (f32, f32, f32),
    pub(crate) from_right: (f32, f32, f32),
    pub(crate) to_left: (f32, f32, f32),
    pub(crate) to_right: (f32, f32, f32),
}

impl Ramp {
    fn on_ramp(&self, point: Vec2) -> bool {
        let corner = |(x, y, _): (f32, f32, f32)| Vec2::new(x, y);
        let quad = [
            corner(self.from_left),
            corner(self.from_right),
            corner(self.to_right),
            corner(self.to_left),
        ];
        point_in_polygon(&quad, point)
    }

    fn height_at(&self, point: Vec2) -> Option<f32> {
        let (p0x, p0y, p0z) = self.from_left;
        let (p1x, p1y, p1z) = self.from_right;
        let (p2x, p2y, p2z) = self.to_left;
        let u = (p1x - p0x, p1y - p0y, p1z - p0z);
        let v = (p2x - p0x, p2y - p0y, p2z - p0z);
        let normal = (
            u.1 * v.2 - u.2 * v.1,
            u.2 * v.0 - u.0 * v.2,
            u.0 * v.1 - u.1 * v.0,
        );
        if normal.2.abs() <= f32::EPSILON {
            return None;
        }
        Some(p0z - (normal.0 * (point.x - p0x) + normal.1 * (point.y - p0y)) / normal.2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndCellLink {
    OwnTag(u8),
    Ramp,
}

#[derive(Debug, Clone, PartialEq)]
struct RampStamp {
    entity: Entity,
    ramp: Ramp,
    end_links: Vec<(Cell, EndCellLink)>,
    released: Vec<Cell>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WallLayer {
    stamps: Vec<Stamp>,
    tags: Vec<u8>,
    connect: Vec<u8>,
    heights: Vec<Option<f32>>,
    ramps: Vec<RampStamp>,
    rim: Vec<u8>,
    released: Vec<u8>,
    side: Vec<u8>,
    generation: u64,
}

impl WallLayer {
    pub(crate) fn empty(cell_count: usize) -> Self {
        Self {
            stamps: Vec::new(),
            tags: vec![GROUND_TAG; cell_count],
            connect: vec![0; cell_count],
            heights: vec![None; MAX_WALL_TAGS],
            ramps: Vec::new(),
            rim: vec![0; cell_count],
            released: vec![0; cell_count],
            side: vec![0; cell_count],
            generation: 0,
        }
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn stamp_ramp(
        &mut self,
        geometry: CellGeometry,
        extent: GridExtent,
        entity: Entity,
        ramp: Ramp,
        heights: &HeightField,
        released: &[Cell],
    ) {
        self.generation = self.generation.wrapping_add(1);
        let end_links = self.resolve_ramp_ends(geometry, extent, &ramp, heights);
        let at = self
            .ramps
            .partition_point(|held| held.entity.index() < entity.index());
        self.ramps.insert(
            at,
            RampStamp {
                entity,
                ramp,
                end_links,
                released: released.to_vec(),
            },
        );
        self.rebuild(geometry, extent);
    }

    fn ramp_at(&self, point: Vec2) -> Option<&Ramp> {
        self.ramps
            .iter()
            .find(|held| held.ramp.on_ramp(point))
            .map(|held| &held.ramp)
    }

    pub(crate) fn tag_at(&self, extent: GridExtent, cell: Cell) -> Layer {
        let Some(index) = extent.index(cell).and_then(|raw| usize::try_from(raw).ok()) else {
            return Layer::GROUND;
        };
        Layer(self.tags.get(index).copied().unwrap_or(GROUND_TAG))
    }

    pub(crate) fn tag_height(&self, tag: Layer) -> Option<f32> {
        if !tag.is_wall_top() {
            return None;
        }
        let k = usize::from(tag.0.checked_sub(Layer::FIRST_WALL)?);
        self.heights.get(k).copied().flatten()
    }

    pub(crate) fn surface_height(
        &self,
        geometry: CellGeometry,
        extent: GridExtent,
        point: Vec2,
    ) -> Option<f32> {
        if let Some(height) = self.ramp_at(point).and_then(|ramp| ramp.height_at(point)) {
            return Some(height);
        }
        let cell = geometry.cell_at(point, 0, extent)?;
        self.tag_height(self.tag_at(extent, cell))
    }

    pub(crate) fn sticky_layer(
        &self,
        geometry: CellGeometry,
        extent: GridExtent,
        point: Vec2,
        current: Layer,
        z: f32,
    ) -> Layer {
        let on_ramp = self.ramp_at(point).is_some();
        let offered = geometry
            .cell_at(point, 0, extent)
            .map_or(Layer::GROUND, |cell| self.tag_at(extent, cell))
            .0;
        let ramp_row = || {
            if on_ramp {
                Layer::RAMPS
            } else if current == Layer::RAMPS {
                Layer::GROUND
            } else {
                current
            }
        };
        match offered {
            RAMP_TAG => ramp_row(),
            GROUND_TAG => {
                if current.is_wall_top() {
                    ramp_row()
                } else if current == Layer::RAMPS && on_ramp {
                    current
                } else {
                    Layer::GROUND
                }
            }
            tag if tag >= FIRST_WALL_TAG => {
                let within_tolerance = self
                    .tag_height(Layer(tag))
                    .is_some_and(|height| (height - z).abs() <= STICKY_LAYER_TOLERANCE);
                if current == Layer::GROUND || !within_tolerance {
                    ramp_row()
                } else if on_ramp {
                    Layer::RAMPS
                } else {
                    Layer(tag)
                }
            }
            other => {
                if current == Layer::RAMPS && on_ramp {
                    current
                } else {
                    Layer(other)
                }
            }
        }
    }

    pub(crate) fn wall_height(
        &self,
        geometry: CellGeometry,
        extent: GridExtent,
        point: Vec2,
        layer: Layer,
        ground: Option<f32>,
    ) -> Option<f32> {
        if layer == Layer::GROUND {
            return ground;
        }
        let new_layer = self.layer_at(geometry, extent, point);
        if new_layer == Layer::RAMPS || new_layer.0 == layer.0 {
            return self.surface_height(geometry, extent, point).or(ground);
        }
        self.tag_height(new_layer).or(ground)
    }

    pub(crate) fn layer_at(
        &self,
        geometry: CellGeometry,
        extent: GridExtent,
        point: Vec2,
    ) -> Layer {
        if self.ramp_at(point).is_some() {
            return Layer::RAMPS;
        }
        let Some(cell) = geometry.cell_at(point, 0, extent) else {
            return Layer::GROUND;
        };
        self.tag_at(extent, cell)
    }

    pub(crate) fn stamp(
        &mut self,
        geometry: CellGeometry,
        extent: GridExtent,
        entity: Entity,
        polygon: Polygon,
        counts: &[u8],
    ) {
        self.generation = self.generation.wrapping_add(1);
        let released = self.touched_live_obstacles(geometry, extent, &polygon, counts);
        let at = self
            .stamps
            .partition_point(|held| held.entity.index() < entity.index());
        if self
            .stamps
            .get(at)
            .is_some_and(|held| held.entity.index() == entity.index())
        {
            if let Some(slot) = self.stamps.get_mut(at) {
                *slot = Stamp {
                    entity,
                    polygon,
                    released,
                };
            }
        } else {
            self.stamps.insert(
                at,
                Stamp {
                    entity,
                    polygon,
                    released,
                },
            );
        }
        self.rebuild(geometry, extent);
    }

    fn touched_live_obstacles(
        &self,
        geometry: CellGeometry,
        extent: GridExtent,
        polygon: &Polygon,
        counts: &[u8],
    ) -> Vec<Cell> {
        let Some(cells) = self.covering_cells(geometry, extent, polygon) else {
            return Vec::new();
        };
        cells
            .into_iter()
            .filter(|(_, corners)| *corners > 0)
            .filter_map(|(cell, _)| {
                let index = extent
                    .index(cell)
                    .and_then(|raw| usize::try_from(raw).ok())?;
                (counts.get(index).copied().unwrap_or(0) > 0).then_some(cell)
            })
            .collect()
    }

    pub(crate) fn holds(&self, entity: Entity) -> bool {
        self.stamps
            .binary_search_by_key(&entity.index(), |held| held.entity.index())
            .is_ok()
    }

    pub(crate) fn entity_cells(
        &self,
        geometry: CellGeometry,
        extent: GridExtent,
        entity: Entity,
    ) -> Option<Vec<Cell>> {
        let at = self
            .stamps
            .binary_search_by_key(&entity.index(), |held| held.entity.index())
            .ok()?;
        let polygon = &self.stamps.get(at)?.polygon;
        let cells = self.covering_cells(geometry, extent, polygon)?;
        Some(
            cells
                .into_iter()
                .filter(|(_, corners)| *corners > 0)
                .map(|(cell, _)| cell)
                .collect(),
        )
    }

    fn dead_stamps(&self, world: &World) -> Vec<Entity> {
        let mut all: Vec<Entity> = self
            .stamps
            .iter()
            .map(|stamp| stamp.entity)
            .chain(self.ramps.iter().map(|held| held.entity))
            .collect();
        all.sort_unstable_by_key(|entity| entity.index());
        all.dedup();
        all.into_iter()
            .filter(|entity| !world.is_live(*entity))
            .collect()
    }

    pub(crate) fn unstamp(&mut self, geometry: CellGeometry, extent: GridExtent, entity: Entity) {
        self.generation = self.generation.wrapping_add(1);
        let had_ramp = self.ramps.iter().any(|held| held.entity == entity);
        self.ramps.retain(|held| held.entity != entity);
        let had_stamp = self
            .stamps
            .binary_search_by_key(&entity.index(), |held| held.entity.index())
            .ok();
        if let Some(at) = had_stamp {
            self.stamps.remove(at);
        }
        if had_ramp || had_stamp.is_some() {
            self.rebuild(geometry, extent);
        }
    }

    fn rebuild(&mut self, geometry: CellGeometry, extent: GridExtent) {
        self.generation = self.generation.wrapping_add(1);
        let cell_count = usize::try_from(extent.cell_count()).unwrap_or(0);
        self.tags = vec![GROUND_TAG; cell_count];
        self.connect = vec![0; cell_count];
        self.heights = vec![None; MAX_WALL_TAGS];
        self.side = vec![0; cell_count];
        self.rim = vec![0; cell_count];
        let stamps = self.stamps.clone();
        for stamp in &stamps {
            let tag = self.apply(geometry, extent, &stamp.polygon);
            diag!(
                "wall: entity {} height {:.1} tag {}",
                stamp.entity.index(),
                stamp.polygon.height,
                tag.map_or(0, |tag| tag.0)
            );
        }
        let ramps = self.ramps.clone();
        for ramp in &ramps {
            self.stamp_ramp_cells(geometry, extent, &ramp.ramp, &ramp.end_links);
        }
        self.released = vec![0; cell_count];
        let released_cells = stamps
            .iter()
            .flat_map(|stamp| stamp.released.iter())
            .chain(ramps.iter().flat_map(|ramp| ramp.released.iter()));
        for cell in released_cells {
            if let Some(index) = extent
                .index(*cell)
                .and_then(|raw| usize::try_from(raw).ok())
            {
                if let Some(slot) = self.released.get_mut(index) {
                    *slot = 1;
                }
            }
        }
    }

    pub(crate) fn rim(&self) -> &[u8] {
        &self.rim
    }

    pub(crate) fn released(&self) -> &[u8] {
        &self.released
    }

    pub(crate) fn side(&self) -> &[u8] {
        &self.side
    }

    pub(crate) fn tags(&self) -> &[u8] {
        &self.tags
    }

    pub(crate) fn connect(&self) -> &[u8] {
        &self.connect
    }

    pub(crate) fn stamp_resolved_ramps(
        &mut self,
        geometry: CellGeometry,
        extent: GridExtent,
        entity: Entity,
        heights: &HeightField,
        placed: &[Ramp],
        released: &[Cell],
    ) -> Vec<String> {
        self.generation = self.generation.wrapping_add(1);
        let mut diags = Vec::new();
        for &ramp in placed {
            let end_links = self.resolve_ramp_ends(geometry, extent, &ramp, heights);
            diags.push(ramp_diag_line(entity, &end_links));
            self.stamp_ramp(geometry, extent, entity, ramp, heights, released);
        }
        diags
    }

    fn resolve_ramp_ends(
        &self,
        geometry: CellGeometry,
        extent: GridExtent,
        ramp: &Ramp,
        heights: &HeightField,
    ) -> Vec<(Cell, EndCellLink)> {
        let mut out = Vec::new();
        for (cell, index, corners) in ramp_touched_cells(geometry, extent, ramp) {
            if corners == 4 || ramp_end_of(geometry, ramp, cell).is_none() {
                continue;
            }
            let mut low: Option<(Vec2, f32)> = None;
            for corner in cell_corners(geometry, cell) {
                if let Some(height) = ramp.height_at(corner) {
                    if low.is_none_or(|(_, best)| height < best) {
                        low = Some((corner, height));
                    }
                }
            }
            let Some((low_corner, ramp_height)) = low else {
                out.push((cell, EndCellLink::Ramp));
                continue;
            };
            let tag = self.tags.get(index).copied().unwrap_or(GROUND_TAG);
            let surface_height = if tag == GROUND_TAG {
                heights.height_at_point(low_corner)
            } else {
                self.tag_height(Layer(tag))
            };
            let link = match surface_height {
                Some(height) if (height - ramp_height).abs() < RAMP_END_TOLERANCE => {
                    EndCellLink::OwnTag(tag)
                }
                _ => EndCellLink::Ramp,
            };
            out.push((cell, link));
        }
        out
    }

    fn stamp_ramp_cells(
        &mut self,
        geometry: CellGeometry,
        extent: GridExtent,
        ramp: &Ramp,
        end_links: &[(Cell, EndCellLink)],
    ) {
        self.generation = self.generation.wrapping_add(1);
        for (cell, index, corners) in ramp_touched_cells(geometry, extent, ramp) {
            if corners == 4 {
                if let Some(slot) = self.tags.get_mut(index) {
                    *slot = RAMP_TAG;
                }
                if let Some(slot) = self.connect.get_mut(index) {
                    *slot = 0;
                }
                continue;
            }
            if ramp_end_of(geometry, ramp, cell).is_none() {
                if let Some(slot) = self.tags.get_mut(index) {
                    *slot = RAMP_TAG;
                }
                if let Some(slot) = self.connect.get_mut(index) {
                    *slot = 0;
                }
                if let Some(slot) = self.side.get_mut(index) {
                    *slot = 1;
                }
                continue;
            }
            let link = end_links
                .iter()
                .find(|(linked, _)| *linked == cell)
                .map_or(EndCellLink::Ramp, |(_, link)| *link);
            match link {
                EndCellLink::OwnTag(_) => {
                    if let Some(slot) = self.connect.get_mut(index) {
                        *slot = RAMP_TAG;
                    }
                    if let Some(slot) = self.rim.get_mut(index) {
                        *slot = 0;
                    }
                }
                EndCellLink::Ramp => {
                    if let Some(slot) = self.tags.get_mut(index) {
                        *slot = RAMP_TAG;
                    }
                    if let Some(slot) = self.connect.get_mut(index) {
                        *slot = 0;
                    }
                }
            }
        }
    }

    fn apply(
        &mut self,
        geometry: CellGeometry,
        extent: GridExtent,
        polygon: &Polygon,
    ) -> Option<Layer> {
        self.generation = self.generation.wrapping_add(1);
        let tag = self.tag_for_height(polygon.height);
        let cells = self.covering_cells(geometry, extent, polygon)?;
        let width = extent.width;
        let height = extent.height;
        let index_of = |cell: Cell| -> Option<usize> {
            extent.index(cell).and_then(|raw| usize::try_from(raw).ok())
        };
        let mut stamped = Vec::with_capacity(cells.len());
        for (cell, corners) in cells {
            if corners == 0 {
                continue;
            }
            let Some(index) = index_of(cell) else {
                continue;
            };
            let current = self.tags.get(index).copied().unwrap_or(GROUND_TAG);
            let keep = current >= FIRST_WALL_TAG
                && self
                    .tag_height(Layer(current))
                    .is_some_and(|existing| existing >= polygon.height);
            if keep {
                continue;
            }
            if let Some(slot) = self.tags.get_mut(index) {
                *slot = tag.0;
            }
            stamped.push(cell);
        }
        let rim: Vec<Cell> = stamped
            .iter()
            .copied()
            .filter(|cell| {
                let different = neighbours(*cell, width, height)
                    .filter(|neighbour| {
                        index_of(*neighbour).is_some_and(|index| {
                            self.tags.get(index).copied().unwrap_or(GROUND_TAG) != tag.0
                        })
                    })
                    .count();
                different > 1
            })
            .collect();
        for cell in rim {
            if let Some(index) = index_of(cell) {
                if let Some(slot) = self.rim.get_mut(index) {
                    *slot = 1;
                }
            }
        }
        Some(tag)
    }

    fn tag_for_height(&mut self, height: f32) -> Layer {
        self.generation = self.generation.wrapping_add(1);
        let mut nearest: Option<(usize, f32)> = None;
        for (k, existing) in self.heights.iter().enumerate() {
            let Some(existing) = existing else { continue };
            let diff = (existing - height).abs();
            if nearest.is_none_or(|(_, best)| diff < best) {
                nearest = Some((k, diff));
            }
        }
        if nearest.is_some_and(|(_, diff)| diff <= HEIGHT_MERGE_TOLERANCE) {
            let (k, _) = nearest.unwrap_or((0, 0.0));
            return self.raise_tag(k, height);
        }
        if let Some(free) = self.heights.iter().position(Option::is_none) {
            if let Some(slot) = self.heights.get_mut(free) {
                *slot = Some(height);
            }
            return Self::tag_of(free);
        }
        let (k, _) = nearest.unwrap_or((0, 0.0));
        self.raise_tag(k, height)
    }

    fn raise_tag(&mut self, k: usize, height: f32) -> Layer {
        self.generation = self.generation.wrapping_add(1);
        if let Some(slot) = self.heights.get_mut(k) {
            *slot = Some(slot.unwrap_or(height).max(height));
        }
        Self::tag_of(k)
    }

    fn tag_of(k: usize) -> Layer {
        Layer(
            u8::try_from(k)
                .ok()
                .and_then(|k| k.checked_add(FIRST_WALL_TAG))
                .unwrap_or(u8::MAX),
        )
    }

    #[allow(clippy::cast_possible_truncation)]
    fn covering_cells(
        &self,
        geometry: CellGeometry,
        extent: GridExtent,
        polygon: &Polygon,
    ) -> Option<Vec<(Cell, u8)>> {
        let (min, max) = polygon_bounds(&polygon.perimeter)?;
        let cell_size = geometry.cell_size();
        let origin = geometry.origin();
        let to_cell = |coordinate: f32, floor: bool| -> i64 {
            let raw = (coordinate - origin.x) / cell_size;
            if floor {
                raw.floor() as i64
            } else {
                raw.ceil() as i64
            }
        };
        let to_cell_y = |coordinate: f32, floor: bool| -> i64 {
            let raw = (coordinate - origin.y) / cell_size;
            if floor {
                raw.floor() as i64
            } else {
                raw.ceil() as i64
            }
        };
        const PAD: i64 = 2;
        let x0 = to_cell(min.x, true).saturating_sub(PAD);
        let x1 = to_cell(max.x, false).saturating_add(PAD);
        let y0 = to_cell_y(min.y, true).saturating_sub(PAD);
        let y1 = to_cell_y(max.y, false).saturating_add(PAD);
        let mut out = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (Ok(cx), Ok(cy)) = (u16::try_from(x.max(0)), u16::try_from(y.max(0))) else {
                    continue;
                };
                let cell = Cell::new(cx, cy);
                if !extent.contains(cell) {
                    continue;
                }
                let corners = cell_corners(geometry, cell)
                    .into_iter()
                    .filter(|corner| point_in_polygon(&polygon.perimeter, *corner))
                    .count();
                let corners = u8::try_from(corners).unwrap_or(4);
                out.push((cell, corners));
            }
        }
        Some(out)
    }
}

#[allow(clippy::cast_possible_truncation)]
fn ramp_touched_cells(
    geometry: CellGeometry,
    extent: GridExtent,
    ramp: &Ramp,
) -> Vec<(Cell, usize, u8)> {
    let quad = [ramp.from_left, ramp.from_right, ramp.to_right, ramp.to_left]
        .map(|(x, y, _)| Vec2::new(x, y));
    let Some((min, max)) = polygon_bounds(&quad) else {
        return Vec::new();
    };
    let cell_size = geometry.cell_size();
    let origin = geometry.origin();
    let to_cell = |value: f32, from: f32, floor: bool| -> i64 {
        let raw = (value - from) / cell_size;
        if floor {
            raw.floor() as i64
        } else {
            raw.ceil() as i64
        }
    };
    let (x0, x1) = (
        to_cell(min.x, origin.x, true),
        to_cell(max.x, origin.x, false),
    );
    let (y0, y1) = (
        to_cell(min.y, origin.y, true),
        to_cell(max.y, origin.y, false),
    );
    let mut out = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (Ok(cx), Ok(cy)) = (u16::try_from(x.max(0)), u16::try_from(y.max(0))) else {
                continue;
            };
            let cell = Cell::new(cx, cy);
            if !extent.contains(cell) {
                continue;
            }
            let Some(index) = extent.index(cell).and_then(|raw| usize::try_from(raw).ok()) else {
                continue;
            };
            let corners = cell_corners(geometry, cell)
                .into_iter()
                .filter(|corner| point_in_polygon(&quad, *corner))
                .count();
            if corners == 0 {
                continue;
            }
            out.push((cell, index, u8::try_from(corners).unwrap_or(4)));
        }
    }
    out
}

fn ramp_end_of(geometry: CellGeometry, ramp: &Ramp, cell: Cell) -> Option<usize> {
    let corners = cell_corners(geometry, cell);
    let (min, max) = (corners[0], corners[3]);
    let inset = geometry.cell_size();
    for end in 0..2 {
        let (a, b) = end_segment(ramp, end);
        let (ia, ib) = inset_segment(a, b, inset);
        if segment_crosses_rect(ia, ib, min, max) {
            return Some(end);
        }
    }
    None
}

fn end_segment(ramp: &Ramp, end: usize) -> (Vec2, Vec2) {
    let to2 = |(x, y, _): (f32, f32, f32)| Vec2::new(x, y);
    if end == 0 {
        (to2(ramp.from_left), to2(ramp.from_right))
    } else {
        (to2(ramp.to_left), to2(ramp.to_right))
    }
}

fn inset_segment(a: Vec2, b: Vec2, inset: f32) -> (Vec2, Vec2) {
    let dir = b.minus(a);
    let len = dir.length();
    if len <= f32::EPSILON {
        return (a, b);
    }
    let unit = dir.scale(1.0 / len);
    let shrink = inset.min(len * 0.5);
    (a.plus(unit.scale(shrink)), b.minus(unit.scale(shrink)))
}

fn segment_crosses_rect(p0: Vec2, p1: Vec2, min: Vec2, max: Vec2) -> bool {
    let d = p1.minus(p0);
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    let edges = [
        (-d.x, p0.x - min.x),
        (d.x, max.x - p0.x),
        (-d.y, p0.y - min.y),
        (d.y, max.y - p0.y),
    ];
    for (p, q) in edges {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return false;
            }
            if r > t0 {
                t0 = r;
            }
        } else if r < t0 {
            return false;
        } else if r < t1 {
            t1 = r;
        }
    }
    t0 <= t1
}

fn ramp_diag_line(entity: Entity, end_links: &[(Cell, EndCellLink)]) -> String {
    let mut by_tag: Vec<(u8, u32)> = Vec::new();
    let mut plain = 0u32;
    for (_, link) in end_links {
        match link {
            EndCellLink::OwnTag(tag) => {
                if let Some(entry) = by_tag.iter_mut().find(|(seen, _)| seen == tag) {
                    entry.1 = entry.1.saturating_add(1);
                } else {
                    by_tag.push((*tag, 1));
                }
            }
            EndCellLink::Ramp => plain = plain.saturating_add(1),
        }
    }
    by_tag.sort_unstable_by_key(|(tag, _)| *tag);
    let mut parts: Vec<String> = by_tag
        .into_iter()
        .map(|(tag, count)| format!("tag {tag} x{count}"))
        .collect();
    if plain > 0 {
        parts.push(format!("{plain} plain"));
    }
    format!(
        "wall: ramp on entity {} stamped, end-line cells connect: {}",
        entity.index(),
        if parts.is_empty() {
            "none".to_string()
        } else {
            parts.join(", ")
        }
    )
}

fn cell_corners(geometry: CellGeometry, cell: Cell) -> [Vec2; 4] {
    let size = geometry.cell_size();
    let origin = geometry.origin();
    let x0 = origin.x + f32::from(cell.x) * size;
    let y0 = origin.y + f32::from(cell.y) * size;
    let x1 = x0 + size;
    let y1 = y0 + size;
    [
        Vec2::new(x0, y0),
        Vec2::new(x1, y0),
        Vec2::new(x0, y1),
        Vec2::new(x1, y1),
    ]
}

fn neighbours(cell: Cell, width: u16, height: u16) -> impl Iterator<Item = Cell> {
    let (cx, cy) = (i32::from(cell.x), i32::from(cell.y));
    (-1..=1).flat_map(move |dy| {
        (-1..=1).filter_map(move |dx| {
            if dx == 0 && dy == 0 {
                return None;
            }
            let (Ok(x), Ok(y)) = (
                u16::try_from(cx.saturating_add(dx)),
                u16::try_from(cy.saturating_add(dy)),
            ) else {
                return None;
            };
            (x < width && y < height).then(|| Cell::new(x, y))
        })
    })
}

fn polygon_bounds(points: &[Vec2]) -> Option<(Vec2, Vec2)> {
    let mut min = Vec2::new(f32::MAX, f32::MAX);
    let mut max = Vec2::new(f32::MIN, f32::MIN);
    let mut any = false;
    for point in points {
        if !point.x.is_finite() || !point.y.is_finite() {
            continue;
        }
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
        any = true;
    }
    any.then_some((min, max))
}

fn point_in_polygon(points: &[Vec2], point: Vec2) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (Some(a), Some(b)) = (points.get(i), points.get(j)) else {
            continue;
        };
        let crosses = (a.y > point.y) != (b.y > point.y);
        if crosses {
            let denom = b.y - a.y;
            if denom != 0.0 {
                let x_at = a.x + (point.y - a.y) * (b.x - a.x) / denom;
                if point.x < x_at {
                    inside = !inside;
                }
            }
        }
        j = i;
    }
    inside
}

pub(crate) fn layer_of(world: &World, id: ComponentId<WallLayer>) -> Option<&WallLayer> {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
}

pub(crate) fn rim_of(world: &World, id: ComponentId<WallLayer>) -> &[u8] {
    layer_of(world, id).map_or(&[][..], WallLayer::rim)
}

pub(crate) fn tags_of(world: &World, id: ComponentId<WallLayer>) -> &[u8] {
    layer_of(world, id).map_or(&[][..], WallLayer::tags)
}

pub(crate) fn connect_of(world: &World, id: ComponentId<WallLayer>) -> &[u8] {
    layer_of(world, id).map_or(&[][..], WallLayer::connect)
}

pub(crate) fn released_of(world: &World, id: ComponentId<WallLayer>) -> &[u8] {
    layer_of(world, id).map_or(&[][..], WallLayer::released)
}

pub(crate) fn side_of(world: &World, id: ComponentId<WallLayer>) -> &[u8] {
    layer_of(world, id).map_or(&[][..], WallLayer::side)
}

pub(crate) fn reap_dead_stamps(
    tick: &mut Tick<'_>,
    id: ComponentId<WallLayer>,
    geometry: CellGeometry,
    extent: GridExtent,
) -> mint_sim::Result<()> {
    let Some(clock_entity) = tick
        .state()
        .store(id)
        .and_then(|store| store.entities().first().copied())
    else {
        return Ok(());
    };
    let dead = tick
        .state()
        .get(id, clock_entity)
        .map(|layer| layer.dead_stamps(tick.state()))
        .unwrap_or_default();
    if dead.is_empty() {
        return Ok(());
    }
    let Some(mut layer) = tick.state().get(id, clock_entity).cloned() else {
        return Ok(());
    };
    for entity in dead {
        layer.unstamp(geometry, extent, entity);
    }
    tick.state_mut().insert(id, clock_entity, layer)?;
    Ok(())
}

impl Component for WallLayer {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.stamps.len());
        for stamp in &self.stamps {
            stamp.entity.hash_into(hasher);
            hasher.write_f32(stamp.polygon.height);
            hasher.write_usize(stamp.polygon.perimeter.len());
            for point in &stamp.polygon.perimeter {
                hasher.write_f32(point.x);
                hasher.write_f32(point.y);
            }
            hasher.write_usize(stamp.released.len());
            for cell in &stamp.released {
                hasher.write_u16(cell.x);
                hasher.write_u16(cell.y);
            }
        }
        hasher.write_usize(self.ramps.len());
        for held in &self.ramps {
            held.entity.hash_into(hasher);
            for (x, y, z) in [
                held.ramp.from_left,
                held.ramp.from_right,
                held.ramp.to_left,
                held.ramp.to_right,
            ] {
                hasher.write_f32(x);
                hasher.write_f32(y);
                hasher.write_f32(z);
            }
            hasher.write_usize(held.end_links.len());
            for (cell, link) in &held.end_links {
                hasher.write_u16(cell.x);
                hasher.write_u16(cell.y);
                match link {
                    EndCellLink::Ramp => hasher.write_u8(0),
                    EndCellLink::OwnTag(tag) => {
                        hasher.write_u8(1);
                        hasher.write_u8(*tag);
                    }
                }
            }
            hasher.write_usize(held.released.len());
            for cell in &held.released {
                hasher.write_u16(cell.x);
                hasher.write_u16(cell.y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::World;

    fn geometry_and_extent() -> (CellGeometry, GridExtent) {
        (
            CellGeometry::new(10.0, Vec2::new(0.0, 0.0)).unwrap(),
            GridExtent::new(40, 40).unwrap(),
        )
    }

    fn square(x0: f32, y0: f32, x1: f32, y1: f32, height: f32) -> Polygon {
        Polygon {
            perimeter: vec![
                Vec2::new(x0, y0),
                Vec2::new(x1, y0),
                Vec2::new(x1, y1),
                Vec2::new(x0, y1),
            ],
            height,
        }
    }

    fn cell_tag(layer: &WallLayer, extent: GridExtent, x: u16, y: u16) -> u8 {
        layer.tag_at(extent, Cell::new(x, y)).0
    }

    fn flat_heights(geometry: CellGeometry, extent: GridExtent) -> HeightField {
        let node_extent = GridExtent::new(extent.width + 1, extent.height + 1).unwrap();
        let samples = vec![0.0; usize::from(extent.width + 1) * usize::from(extent.height + 1)];
        HeightField::new(node_extent, geometry, samples).unwrap()
    }

    #[test]
    fn a_square_wall_tops_its_interior_and_rims_its_border() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let entity = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            entity,
            square(100.0, 100.0, 160.0, 130.0, 30.0),
            &[],
        );
        let corner_index = usize::try_from(extent.index(Cell::new(9, 9)).unwrap()).unwrap();
        assert_eq!(layer.rim()[corner_index], 1, "corner is on the border");
        assert_eq!(
            cell_tag(&layer, extent, 9, 9),
            FIRST_WALL_TAG,
            "the rim cell keeps its own true tag underneath (R10)"
        );
        assert_eq!(
            cell_tag(&layer, extent, 13, 11),
            FIRST_WALL_TAG,
            "interior stays wall-top"
        );
        assert_eq!(layer.tag_height(Layer(FIRST_WALL_TAG)), Some(30.0));
    }

    #[test]
    fn hash_into_depends_on_released_cells() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let entity = world.spawn().unwrap();
        let polygon = square(100.0, 100.0, 160.0, 130.0, 30.0);
        let cell_count = usize::try_from(extent.cell_count()).unwrap();

        let empty_counts = vec![0u8; cell_count];
        let mut without_release = WallLayer::empty(cell_count);
        without_release.stamp(geometry, extent, entity, polygon.clone(), &empty_counts);

        let mut live_counts = empty_counts.clone();
        let touched = usize::try_from(extent.index(Cell::new(10, 10)).unwrap()).unwrap();
        live_counts[touched] = 1;
        let mut with_release = WallLayer::empty(cell_count);
        with_release.stamp(geometry, extent, entity, polygon, &live_counts);

        assert_ne!(
            without_release.stamps[0].released.len(),
            with_release.stamps[0].released.len(),
            "fixture must actually produce different released sets"
        );

        let mut hasher_a = StateHasher::new();
        let mut hasher_b = StateHasher::new();
        without_release.hash_into(&mut hasher_a);
        with_release.hash_into(&mut hasher_b);
        assert_ne!(
            hasher_a, hasher_b,
            "released must be part of the hashed state"
        );
    }

    #[test]
    fn an_overlapping_wall_reuses_a_close_tag_but_a_taller_one_gets_its_own() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let a = world.spawn().unwrap();
        let b = world.spawn().unwrap();
        let c = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            a,
            square(100.0, 100.0, 160.0, 130.0, 30.0),
            &[],
        );
        layer.stamp(
            geometry,
            extent,
            b,
            square(110.0, 100.0, 170.0, 130.0, 32.0),
            &[],
        );
        assert_eq!(layer.tag_height(Layer(FIRST_WALL_TAG)), Some(32.0));
        assert_eq!(cell_tag(&layer, extent, 13, 11), FIRST_WALL_TAG);
        layer.stamp(
            geometry,
            extent,
            c,
            square(100.0, 100.0, 170.0, 130.0, 40.0),
            &[],
        );
        assert_eq!(
            cell_tag(&layer, extent, 13, 11),
            FIRST_WALL_TAG + 1,
            "the taller wall wins the overlap"
        );
        assert_eq!(layer.tag_height(Layer(FIRST_WALL_TAG + 1)), Some(40.0));
    }

    #[test]
    fn a_full_tag_table_reuses_and_raises_the_nearest_tag() {
        let mut layer = WallLayer::empty(16);
        for k in 0..MAX_WALL_TAGS {
            let tag = layer.tag_for_height(k as f32 * 100.0);
            assert_eq!(tag.0, FIRST_WALL_TAG + u8::try_from(k).unwrap_or(u8::MAX));
        }
        let tag = layer.tag_for_height(6400.0);
        assert_eq!(tag.0, FIRST_WALL_TAG + 63, "reuses the nearest tag, 63");
        assert_eq!(layer.tag_height(Layer(FIRST_WALL_TAG + 63)), Some(6400.0));
    }

    #[test]
    fn unstamping_a_wall_restores_ground() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let entity = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            entity,
            square(100.0, 100.0, 160.0, 130.0, 30.0),
            &[],
        );
        assert_eq!(cell_tag(&layer, extent, 13, 11), FIRST_WALL_TAG);
        layer.unstamp(geometry, extent, entity);
        assert_eq!(cell_tag(&layer, extent, 13, 11), GROUND_TAG);
        assert_eq!(cell_tag(&layer, extent, 10, 10), GROUND_TAG);
        assert_eq!(layer.tag_height(Layer(FIRST_WALL_TAG)), None);
    }

    #[test]
    fn a_ground_unit_in_a_wall_tagged_edge_cell_stays_on_the_ground() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let entity = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            entity,
            square(100.0, 100.0, 160.0, 130.0, 30.0),
            &[],
        );
        let edge_point = Vec2::new(100.0, 100.0);
        assert_eq!(
            cell_tag(&layer, extent, 10, 10),
            FIRST_WALL_TAG,
            "the edge cell itself reads the wall's own tag"
        );
        let resolved = layer.sticky_layer(geometry, extent, edge_point, Layer::GROUND, 0.0);
        assert_eq!(resolved, Layer::GROUND, "a ground unit stays on the ground");
    }

    #[test]
    fn only_a_ramp_layer_picks_up_a_wall_top_at_the_right_height() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let entity = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            entity,
            square(100.0, 100.0, 160.0, 130.0, 30.0),
            &[],
        );
        let top_point = Vec2::new(130.0, 115.0);
        let resolved = layer.sticky_layer(geometry, extent, top_point, Layer::RAMPS, 30.0);
        assert_eq!(resolved, Layer(FIRST_WALL_TAG));
        let far_below = layer.sticky_layer(geometry, extent, top_point, Layer::RAMPS, 0.0);
        assert_eq!(
            far_below,
            Layer::GROUND,
            "outside the 10.0 tolerance and off any ramp object, a ramp-layer unit becomes ground"
        );
        let from_ground = layer.sticky_layer(geometry, extent, top_point, Layer::GROUND, 30.0);
        assert_eq!(
            from_ground,
            Layer::GROUND,
            "a ground unit never picks up a wall top without a ramp"
        );
    }

    #[test]
    fn a_ground_unit_inside_a_ramp_object_on_a_ground_tagged_cell_stays_ground() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let ramp = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let heights = flat_heights(geometry, extent);
        layer.stamp_ramp(
            geometry,
            extent,
            ramp,
            Ramp {
                from_left: (95.0, 100.0, 0.0),
                from_right: (95.0, 112.0, 0.0),
                to_left: (125.0, 100.0, 30.0),
                to_right: (125.0, 112.0, 30.0),
            },
            &heights,
            &[],
        );
        let mut probed = 0;
        for x in 95..125u8 {
            for y in 100..112u8 {
                let point = Vec2::new(f32::from(x) + 0.5, f32::from(y) + 0.5);
                let Some(cell) = geometry.cell_at(point, 0, extent) else {
                    continue;
                };
                if layer.ramp_at(point).is_none() || layer.tag_at(extent, cell) != Layer::GROUND {
                    continue;
                }
                probed += 1;
                assert_eq!(
                    layer.sticky_layer(geometry, extent, point, Layer::GROUND, 0.0),
                    Layer::GROUND,
                    "{point:?}"
                );
                assert_eq!(
                    layer.sticky_layer(geometry, extent, point, Layer::RAMPS, 0.0),
                    Layer::RAMPS,
                    "{point:?}"
                );
            }
        }
        assert!(probed > 0, "the ramp left no ground-tagged cell under it");
    }

    #[test]
    fn wall_height_follows_the_resolved_layer_not_the_raw_cell() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let entity = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            entity,
            square(100.0, 100.0, 160.0, 130.0, 30.0),
            &[],
        );
        let edge_point = Vec2::new(100.0, 100.0);
        assert_eq!(
            layer.wall_height(geometry, extent, edge_point, Layer::GROUND, Some(0.0)),
            Some(0.0),
            "a ground layer reads terrain, not the wall's own height"
        );
        assert_eq!(
            layer.wall_height(
                geometry,
                extent,
                edge_point,
                Layer(FIRST_WALL_TAG),
                Some(0.0)
            ),
            Some(30.0),
            "the wall layer reads the wall's own height"
        );
    }

    #[test]
    fn stamping_a_wall_top_releases_only_the_cells_with_a_live_obstacle() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let entity = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let mut counts = vec![0u8; usize::try_from(extent.cell_count()).unwrap()];
        let live_cell = Cell::new(13, 11);
        let quiet_cell = Cell::new(10, 10);
        let live_index = usize::try_from(extent.index(live_cell).unwrap()).unwrap();
        counts[live_index] = 1;
        layer.stamp(
            geometry,
            extent,
            entity,
            square(100.0, 100.0, 160.0, 130.0, 30.0),
            &counts,
        );
        let released = layer.released();
        assert_eq!(released[live_index], 1, "the live cell is released");
        let quiet_index = usize::try_from(extent.index(quiet_cell).unwrap()).unwrap();
        assert_eq!(
            released[quiet_index], 0,
            "a cell with no live count stays unreleased"
        );
    }

    #[test]
    fn only_tags_17_and_up_are_wall_top() {
        assert!(!Layer::GROUND.is_wall_top());
        assert!(!Layer::RAMPS.is_wall_top());
        assert!(Layer(FIRST_WALL_TAG).is_wall_top());
    }

    #[test]
    fn a_ramp_reads_as_ramp_layer_and_interpolates_height() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let wall = world.spawn().unwrap();
        let ramp = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            wall,
            square(100.0, 100.0, 160.0, 130.0, 30.0),
            &[],
        );
        let heights = flat_heights(geometry, extent);
        layer.stamp_ramp(
            geometry,
            extent,
            ramp,
            Ramp {
                from_left: (90.0, 100.0, 0.0),
                from_right: (90.0, 110.0, 0.0),
                to_left: (100.0, 100.0, 30.0),
                to_right: (100.0, 110.0, 30.0),
            },
            &heights,
            &[],
        );
        let mid = Vec2::new(95.0, 105.0);
        assert_eq!(layer.layer_at(geometry, extent, mid), Layer::RAMPS);
        assert_eq!(layer.surface_height(geometry, extent, mid), Some(15.0));
        let elsewhere = Vec2::new(0.0, 0.0);
        assert_eq!(layer.layer_at(geometry, extent, elsewhere), Layer::GROUND);
    }

    #[test]
    fn a_ramp_side_cell_is_tagged_and_marked_but_an_end_cell_is_not() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let ramp = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let heights = flat_heights(geometry, extent);
        layer.stamp_ramp(
            geometry,
            extent,
            ramp,
            Ramp {
                from_left: (70.0, 100.0, 0.0),
                from_right: (70.0, 125.0, 0.0),
                to_left: (130.0, 100.0, 30.0),
                to_right: (130.0, 125.0, 30.0),
            },
            &heights,
            &[],
        );
        let side_index = usize::try_from(extent.index(Cell::new(9, 12)).unwrap()).unwrap();
        assert_eq!(
            cell_tag(&layer, extent, 9, 12),
            RAMP_TAG,
            "tagged like any ramp cell"
        );
        assert_eq!(layer.side()[side_index], 1, "marked as a side cell");
        let end_index = usize::try_from(extent.index(Cell::new(6, 11)).unwrap()).unwrap();
        assert_eq!(
            layer.side()[end_index],
            0,
            "an end-zone cell is not a side cell"
        );
        let interior_index = usize::try_from(extent.index(Cell::new(10, 11)).unwrap()).unwrap();
        assert_eq!(
            layer.side()[interior_index],
            0,
            "the ramp's own interior is not a side cell"
        );
    }

    #[test]
    fn a_ramp_end_over_both_ground_and_a_wall_top_connects_both() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let wall = world.spawn().unwrap();
        let ramp = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            wall,
            square(100.0, 100.0, 160.0, 130.0, 20.0),
            &[],
        );
        let heights = flat_heights(geometry, extent);
        layer.stamp_ramp(
            geometry,
            extent,
            ramp,
            Ramp {
                from_left: (40.0, 70.0, 10.0),
                from_right: (40.0, 160.0, 10.0),
                to_left: (97.0, 70.0, 10.0),
                to_right: (97.0, 160.0, 10.0),
            },
            &heights,
            &[],
        );
        let ground_cell = usize::try_from(extent.index(Cell::new(9, 8)).unwrap()).unwrap();
        let wall_cell = usize::try_from(extent.index(Cell::new(9, 11)).unwrap()).unwrap();
        assert_eq!(
            cell_tag(&layer, extent, 9, 8),
            GROUND_TAG,
            "the ground end-line cell keeps its own tag"
        );
        assert_eq!(
            layer.connect()[ground_cell],
            RAMP_TAG,
            "and connects to the ramp"
        );
        assert_eq!(
            cell_tag(&layer, extent, 9, 11),
            FIRST_WALL_TAG,
            "the wall end-line cell keeps its own tag too"
        );
        assert_eq!(
            layer.connect()[wall_cell],
            RAMP_TAG,
            "and also connects to the ramp - both at the same end"
        );
    }

    #[test]
    fn a_ramp_end_over_two_different_wall_tags_connects_both() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let low = world.spawn().unwrap();
        let high = world.spawn().unwrap();
        let ramp = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            low,
            square(100.0, 100.0, 160.0, 120.0, 17.0),
            &[],
        );
        layer.stamp(
            geometry,
            extent,
            high,
            square(100.0, 120.0, 160.0, 140.0, 25.0),
            &[],
        );
        let heights = flat_heights(geometry, extent);
        layer.stamp_ramp(
            geometry,
            extent,
            ramp,
            Ramp {
                from_left: (40.0, 100.0, 0.0),
                from_right: (40.0, 140.0, 0.0),
                to_left: (97.0, 100.0, 20.0),
                to_right: (97.0, 140.0, 20.0),
            },
            &heights,
            &[],
        );
        let low_cell = usize::try_from(extent.index(Cell::new(9, 10)).unwrap()).unwrap();
        let high_cell = usize::try_from(extent.index(Cell::new(9, 13)).unwrap()).unwrap();
        assert_eq!(cell_tag(&layer, extent, 9, 10), FIRST_WALL_TAG);
        assert_eq!(layer.connect()[low_cell], RAMP_TAG, "tag 17 connects");
        assert_eq!(cell_tag(&layer, extent, 9, 13), FIRST_WALL_TAG + 1);
        assert_eq!(layer.connect()[high_cell], RAMP_TAG, "tag 18 connects too");
    }

    #[test]
    fn a_ramp_end_matching_nothing_within_tolerance_is_plain_ramp() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let ramp = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let heights = flat_heights(geometry, extent);
        layer.stamp_ramp(
            geometry,
            extent,
            ramp,
            Ramp {
                from_left: (40.0, 90.0, 50.0),
                from_right: (40.0, 140.0, 50.0),
                to_left: (97.0, 90.0, 50.0),
                to_right: (97.0, 140.0, 50.0),
            },
            &heights,
            &[],
        );
        let end_cell = usize::try_from(extent.index(Cell::new(9, 11)).unwrap()).unwrap();
        assert_eq!(
            cell_tag(&layer, extent, 9, 11),
            RAMP_TAG,
            "no surface here is within tolerance, so it is plain ramp"
        );
        assert_eq!(layer.connect()[end_cell], 0, "and connects to nothing");
    }

    #[test]
    fn a_ramps_rim_cut_survives_an_unrelated_walls_rebuild() {
        let (geometry, extent) = geometry_and_extent();
        let mut world = World::new();
        let wall = world.spawn().unwrap();
        let ramp = world.spawn().unwrap();
        let other = world.spawn().unwrap();
        let mut layer = WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        layer.stamp(
            geometry,
            extent,
            wall,
            square(100.0, 100.0, 160.0, 130.0, 30.0),
            &[],
        );
        let placed = Ramp {
            from_left: (73.0, 100.0, 0.0),
            from_right: (73.0, 110.0, 0.0),
            to_left: (97.0, 100.0, 30.0),
            to_right: (97.0, 110.0, 30.0),
        };
        let heights = flat_heights(geometry, extent);
        layer.stamp_ramp(geometry, extent, ramp, placed, &heights, &[]);
        assert_eq!(
            cell_tag(&layer, extent, 9, 10),
            FIRST_WALL_TAG,
            "the rim is cut here"
        );
        assert_eq!(
            layer.connect()[usize::try_from(extent.index(Cell::new(9, 10)).unwrap()).unwrap()],
            RAMP_TAG,
            "the opened rim cell connects to the ramp"
        );
        layer.stamp(
            geometry,
            extent,
            other,
            square(500.0, 500.0, 520.0, 520.0, 10.0),
            &[],
        );
        assert_eq!(
            cell_tag(&layer, extent, 9, 10),
            FIRST_WALL_TAG,
            "the cut must still be open after an unrelated rebuild"
        );
    }
}
