use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Cell {
    pub layer: u8,
    pub y: u16,
    pub x: u16,
}

impl Cell {
    #[must_use]
    pub const fn new(x: u16, y: u16) -> Self {
        Self { layer: 0, y, x }
    }

    #[must_use]
    pub const fn on_layer(x: u16, y: u16, layer: u8) -> Self {
        Self { layer, y, x }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GridExtent {
    pub width: u16,
    pub height: u16,
    pub layers: u8,
}

pub const MAX_GRID_CELLS: u32 = 4_000_000;

impl GridExtent {
    pub fn new(width: u16, height: u16) -> Result<Self> {
        Self::layered(width, height, 1)
    }

    pub fn layered(width: u16, height: u16, layers: u8) -> Result<Self> {
        if width == 0 || height == 0 || layers == 0 {
            return Err(Error::EmptyGrid);
        }
        let cells = u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(u64::from(layers));
        if cells > u64::from(MAX_GRID_CELLS) {
            return Err(Error::GridTooLarge {
                cells,
                limit: MAX_GRID_CELLS,
            });
        }
        Ok(Self {
            width,
            height,
            layers,
        })
    }

    #[must_use]
    pub fn cell_count(self) -> u32 {
        u32::from(self.width)
            .saturating_mul(u32::from(self.height))
            .saturating_mul(u32::from(self.layers))
    }

    #[must_use]
    pub const fn contains(self, cell: Cell) -> bool {
        cell.x < self.width && cell.y < self.height && cell.layer < self.layers
    }

    #[must_use]
    pub fn index(self, cell: Cell) -> Option<u32> {
        if !self.contains(cell) {
            return None;
        }
        let plane = u32::from(self.width).saturating_mul(u32::from(self.height));
        Some(
            u32::from(cell.layer)
                .saturating_mul(plane)
                .saturating_add(u32::from(cell.y).saturating_mul(u32::from(self.width)))
                .saturating_add(u32::from(cell.x)),
        )
    }

    #[must_use]
    pub fn cell_at_index(self, index: u32) -> Option<Cell> {
        if index >= self.cell_count() {
            return None;
        }
        let width = u32::from(self.width);
        let plane = width.saturating_mul(u32::from(self.height));
        let layer = index.checked_div(plane)?;
        let within = index.checked_rem(plane)?;
        let y = within.checked_div(width)?;
        let x = within.checked_rem(width)?;
        Some(Cell {
            layer: u8::try_from(layer).ok()?,
            y: u16::try_from(y).ok()?,
            x: u16::try_from(x).ok()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CellFlags(u32);

impl CellFlags {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(u32::MAX);

    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bit(index: u32) -> Self {
        if index >= 32 {
            Self(0)
        } else {
            Self(1u32 << index)
        }
    }

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct MoverClass {
    pub blocked_by: CellFlags,
    pub requires: CellFlags,
    pub gated_by: CellFlags,
    pub asker: Asker,
    pub excused_by: CellFlags,
    pub crosses_layers: bool,
    pub barred_by: CellFlags,
    pub confined_to: Option<CellRect>,
    pub portal_pass: PortalPass,
    pub line_barred: CellFlags,
    pub straight_tail: u16,
    pub object_flags: CellFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellRect {
    pub x0: u16,
    pub y0: u16,
    pub x1: u16,
    pub y1: u16,
}

impl CellRect {
    #[must_use]
    pub const fn contains(self, cell: Cell) -> bool {
        cell.x >= self.x0 && cell.x < self.x1 && cell.y >= self.y0 && cell.y < self.y1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Asker(u8);

impl Asker {
    pub const NOBODY: Self = Self(u8::MAX);

    #[must_use]
    pub const fn new(id: u8) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Default for Asker {
    fn default() -> Self {
        Self::NOBODY
    }
}

impl MoverClass {
    #[must_use]
    pub const fn new(blocked_by: CellFlags) -> Self {
        Self {
            blocked_by,
            requires: CellFlags::NONE,
            gated_by: CellFlags::NONE,
            asker: Asker::NOBODY,
            excused_by: CellFlags::NONE,
            crosses_layers: false,
            barred_by: CellFlags::NONE,
            confined_to: None,
            portal_pass: PortalPass::NONE,
            line_barred: CellFlags::NONE,
            straight_tail: 0,
            object_flags: CellFlags::NONE,
        }
    }

    #[must_use]
    pub const fn object_stamped(self, object_flags: CellFlags) -> Self {
        Self {
            object_flags,
            ..self
        }
    }

    #[must_use]
    pub const fn objects_only(self) -> Self {
        Self {
            blocked_by: CellFlags::from_bits(self.blocked_by.bits() & self.object_flags.bits()),
            requires: CellFlags::NONE,
            excused_by: CellFlags::NONE,
            ..self
        }
    }

    #[must_use]
    pub const fn line_barring(self, line_barred: CellFlags) -> Self {
        Self {
            line_barred,
            ..self
        }
    }

    #[must_use]
    pub const fn straight_tailed(self, cells: u16) -> Self {
        Self {
            straight_tail: cells,
            ..self
        }
    }

    #[must_use]
    pub const fn passing(self, portal_pass: PortalPass) -> Self {
        Self {
            portal_pass,
            ..self
        }
    }

    #[must_use]
    pub const fn restricted(self, barred_by: CellFlags, confined_to: Option<CellRect>) -> Self {
        Self {
            barred_by: self.barred_by.union(barred_by),
            confined_to,
            ..self
        }
    }

    #[must_use]
    pub const fn requiring(self, requires: CellFlags) -> Self {
        Self { requires, ..self }
    }

    #[must_use]
    pub const fn excused(self, excused_by: CellFlags) -> Self {
        Self { excused_by, ..self }
    }

    #[must_use]
    pub const fn asking(self, gated_by: CellFlags, asker: Asker) -> Self {
        Self {
            gated_by,
            asker,
            ..self
        }
    }

    #[must_use]
    pub const fn crossing_layers(self) -> Self {
        Self {
            crosses_layers: true,
            ..self
        }
    }

    #[must_use]
    pub const fn without_crossing_layers(self) -> Self {
        Self {
            crosses_layers: false,
            ..self
        }
    }

    #[must_use]
    pub const fn terrain(self) -> Self {
        Self {
            gated_by: CellFlags::NONE,
            asker: Asker::NOBODY,
            barred_by: CellFlags::NONE,
            confined_to: None,
            portal_pass: PortalPass::NONE,
            straight_tail: 0,
            object_flags: CellFlags::NONE,
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Leg {
    #[default]
    Through,
    Walk,
    Climb,
    Bare,
    Halt,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Portal {
    pub id: u32,
    pub kind: u8,
    pub leg: Leg,
    pub rise: f32,
}

impl Portal {
    #[must_use]
    pub const fn new(id: u32, kind: u8) -> Self {
        Self {
            id,
            kind,
            leg: Leg::Through,
            rise: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortalNode {
    pub position: crate::geom::Vec2,
    pub tag: Option<Portal>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortalLink {
    pub entry: Cell,
    pub exit: Cell,
    pub slot: u8,
    pub nodes: Vec<PortalNode>,
    pub after: Option<Portal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct PortalPass(u128);

impl PortalPass {
    pub const NONE: Self = Self(0);
    pub const SLOTS: u8 = 128;

    #[must_use]
    pub const fn with(self, slot: u8) -> Self {
        if slot >= Self::SLOTS {
            return self;
        }
        Self(self.0 | (1u128 << slot))
    }

    #[must_use]
    pub const fn has(self, slot: u8) -> bool {
        slot < Self::SLOTS && (self.0 >> slot) & 1 == 1
    }
}

pub trait Passability {
    fn extent(&self) -> GridExtent;

    fn flags(&self, cell: Cell) -> CellFlags;

    fn extra_cost(&self, cell: Cell) -> u32;

    fn goal_extra_cost(&self, _cell: Cell, _goal: Option<Cell>) -> u32 {
        0
    }

    fn step_extra_cost(&self, _from: Cell, _to: Cell) -> u32 {
        0
    }

    fn is_passable(&self, cell: Cell, mover: MoverClass) -> bool {
        if !self.extent().contains(cell) {
            return false;
        }
        if mover.confined_to.is_some_and(|rect| !rect.contains(cell)) {
            return false;
        }
        let flags = self.flags(cell);
        if flags.intersects(mover.gated_by) {
            return self.admits(cell, mover.asker)
                && (mover.requires == CellFlags::NONE || flags.intersects(mover.requires));
        }
        if flags.intersects(mover.blocked_by.union(mover.barred_by))
            && !flags.intersects(mover.excused_by)
        {
            return false;
        }
        mover.requires == CellFlags::NONE || flags.intersects(mover.requires)
    }

    fn admits(&self, _cell: Cell, _asker: Asker) -> bool {
        true
    }

    fn line_clear(&self, cell: Cell, mover: MoverClass) -> bool {
        self.is_passable(cell, mover) && !self.flags(cell).intersects(mover.line_barred)
    }

    fn straight_clear(&self, cell: Cell, mover: MoverClass) -> bool {
        self.is_passable(cell, mover)
    }

    fn tail_clear(&self, cell: Cell, mover: MoverClass) -> bool {
        self.line_clear(cell, mover)
    }

    fn portal_at(&self, _cell: Cell) -> Option<Portal> {
        None
    }

    fn portal_links(&self, _cell: Cell) -> &[PortalLink] {
        &[]
    }

    fn admits_step(&self, _from: Cell, _to: Cell, _mover: MoverClass) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct CellData {
    flags: CellFlags,
    extra_cost: u16,
}

#[derive(Debug, Clone)]
pub struct CostGrid {
    extent: GridExtent,
    cells: Vec<CellData>,
}

impl CostGrid {
    #[must_use]
    pub fn new(extent: GridExtent) -> Self {
        let count = usize::try_from(extent.cell_count()).unwrap_or(0);
        Self {
            extent,
            cells: vec![CellData::default(); count],
        }
    }

    #[must_use]
    pub const fn extent(&self) -> GridExtent {
        self.extent
    }

    pub fn set_flags(&mut self, cell: Cell, flags: CellFlags) -> Result<()> {
        self.at_mut(cell)?.flags = flags;
        Ok(())
    }

    pub fn add_flags(&mut self, cell: Cell, flags: CellFlags) -> Result<()> {
        let data = self.at_mut(cell)?;
        data.flags = data.flags.union(flags);
        Ok(())
    }

    pub fn remove_flags(&mut self, cell: Cell, flags: CellFlags) -> Result<()> {
        let data = self.at_mut(cell)?;
        data.flags = CellFlags::from_bits(data.flags.bits() & !flags.bits());
        Ok(())
    }

    pub fn set_extra_cost(&mut self, cell: Cell, cost: u16) -> Result<()> {
        self.at_mut(cell)?.extra_cost = cost;
        Ok(())
    }

    fn at_mut(&mut self, cell: Cell) -> Result<&mut CellData> {
        let out_of_bounds = Error::CellOutOfBounds {
            layer: cell.layer,
            x: cell.x,
            y: cell.y,
        };
        let index = self
            .extent
            .index(cell)
            .and_then(|i| usize::try_from(i).ok())
            .ok_or(out_of_bounds.clone())?;
        self.cells.get_mut(index).ok_or(out_of_bounds)
    }

    fn at(&self, cell: Cell) -> Option<CellData> {
        let index = usize::try_from(self.extent.index(cell)?).ok()?;
        self.cells.get(index).copied()
    }
}

impl Passability for CostGrid {
    fn extent(&self) -> GridExtent {
        self.extent
    }

    fn flags(&self, cell: Cell) -> CellFlags {
        self.at(cell).map_or(CellFlags::NONE, |c| c.flags)
    }

    fn extra_cost(&self, cell: Cell) -> u32 {
        self.at(cell).map_or(0, |c| u32::from(c.extra_cost))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grid_larger_than_the_cap_is_refused_rather_than_allocated() {
        let err = GridExtent::layered(65535, 65535, 255).unwrap_err();
        assert!(matches!(err, Error::GridTooLarge { .. }));
    }

    #[test]
    fn flat_indices_order_the_same_way_cells_do() {
        let extent = GridExtent::layered(7, 5, 3).unwrap();
        let mut previous: Option<Cell> = None;
        for index in 0..extent.cell_count() {
            let cell = extent.cell_at_index(index).unwrap();
            assert_eq!(extent.index(cell), Some(index));
            if let Some(before) = previous {
                assert!(before < cell, "{before:?} should sort before {cell:?}");
            }
            previous = Some(cell);
        }
    }

    #[test]
    fn writing_off_the_edge_of_the_grid_is_an_error() {
        let mut grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
        let err = grid
            .set_flags(Cell::new(4, 0), CellFlags::bit(0))
            .unwrap_err();
        assert!(matches!(err, Error::CellOutOfBounds { x: 4, y: 0, .. }));
    }

    #[test]
    fn without_crossing_layers_clears_only_that_flag() {
        let mover = MoverClass::new(CellFlags::bit(0))
            .requiring(CellFlags::bit(1))
            .crossing_layers();
        let restricted = mover.without_crossing_layers();
        assert!(mover.crosses_layers);
        assert!(!restricted.crosses_layers);
        assert_eq!(restricted.blocked_by, mover.blocked_by);
        assert_eq!(restricted.requires, mover.requires);
    }

    #[test]
    fn cells_outside_the_grid_are_impassable() {
        let grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
        let mover = MoverClass::new(CellFlags::bit(0));
        assert!(grid.is_passable(Cell::new(3, 3), mover));
        assert!(!grid.is_passable(Cell::new(4, 3), mover));
        assert!(!grid.is_passable(Cell::on_layer(3, 3, 1), mover));
    }
}
