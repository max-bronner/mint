use crate::grid::{Cell, GridExtent};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    #[must_use]
    pub fn minus(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }

    #[must_use]
    pub fn scale(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k)
    }

    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[must_use]
    pub fn normalised(self) -> Option<Self> {
        let squared = self.length_squared();
        if !squared.is_finite() || squared <= f32::MIN_POSITIVE {
            return None;
        }
        let length = squared.sqrt();
        Some(Self::new(self.x / length, self.y / length))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Facing(f32);

impl Facing {
    pub const EAST: Self = Self(0.0);

    #[must_use]
    pub fn from_radians(radians: f32) -> Self {
        Self(mint_math::f32::wrap_pi(radians))
    }

    #[must_use]
    pub const fn radians(self) -> f32 {
        self.0
    }

    #[must_use]
    pub fn from_vector(direction: Vec2) -> Option<Self> {
        let squared = direction.length_squared();
        if !squared.is_finite() || squared <= f32::MIN_POSITIVE {
            return None;
        }
        Some(Self(mint_math::f32::atan2(direction.y, direction.x)))
    }

    #[must_use]
    pub fn to_vector(self) -> Vec2 {
        let (sin, cos) = mint_math::f32::sin_cos(self.0);
        Vec2::new(cos, sin)
    }

    #[must_use]
    pub fn delta_to(self, target: Self) -> f32 {
        mint_math::f32::angle_delta(self.0, target.0)
    }

    #[must_use]
    pub fn turned_towards(self, target: Self, max_step: f32) -> Self {
        if !max_step.is_finite() || max_step <= 0.0 {
            return self;
        }
        let delta = self.delta_to(target);
        if delta.abs() <= max_step {
            target
        } else {
            Self::from_radians(self.0 + max_step * delta.signum())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellGeometry {
    cell_size: f32,
    origin: Vec2,
}

impl CellGeometry {
    pub fn new(cell_size: f32, origin: Vec2) -> Result<Self> {
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return Err(Error::InvalidCellSize { cell_size });
        }
        Ok(Self { cell_size, origin })
    }

    #[must_use]
    pub const fn cell_size(self) -> f32 {
        self.cell_size
    }

    #[must_use]
    pub const fn origin(self) -> Vec2 {
        self.origin
    }

    #[must_use]
    pub fn centre_of(self, cell: Cell) -> Vec2 {
        Vec2::new(
            self.origin.x + (f32::from(cell.x) + 0.5) * self.cell_size,
            self.origin.y + (f32::from(cell.y) + 0.5) * self.cell_size,
        )
    }

    #[must_use]
    pub fn cell_at(self, point: Vec2, layer: u8, extent: GridExtent) -> Option<Cell> {
        let x = ((point.x - self.origin.x) / self.cell_size).floor();
        let y = ((point.y - self.origin.y) / self.cell_size).floor();
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        if x < 0.0 || y < 0.0 || x >= f32::from(u16::MAX) || y >= f32::from(u16::MAX) {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cell = Cell::on_layer(x as u16, y as u16, layer);
        extent.contains(cell).then_some(cell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::PI;

    #[test]
    fn a_cell_size_of_zero_is_refused() {
        let err = CellGeometry::new(0.0, Vec2::ZERO).unwrap_err();
        assert!(matches!(err, Error::InvalidCellSize { .. }));
    }

    #[test]
    fn a_point_inside_a_cell_maps_back_to_that_cell() {
        let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
        let extent = GridExtent::new(8, 8).unwrap();
        let cell = Cell::new(3, 5);
        let centre = geometry.centre_of(cell);
        assert_eq!(centre, Vec2::new(35.0, 55.0));
        assert_eq!(geometry.cell_at(centre, 0, extent), Some(cell));
        assert_eq!(
            geometry.cell_at(Vec2::new(30.0, 50.0), 0, extent),
            Some(cell)
        );
        assert_eq!(
            geometry.cell_at(Vec2::new(40.0, 50.0), 0, extent),
            Some(Cell::new(4, 5))
        );
    }

    #[test]
    fn a_point_off_the_map_is_reported_rather_than_clamped() {
        let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
        let extent = GridExtent::new(8, 8).unwrap();
        assert_eq!(geometry.cell_at(Vec2::new(-1.0, 5.0), 0, extent), None);
        assert_eq!(geometry.cell_at(Vec2::new(80.0, 5.0), 0, extent), None);
        assert_eq!(geometry.cell_at(Vec2::new(f32::NAN, 5.0), 0, extent), None);
    }

    #[test]
    fn turning_takes_the_short_way_round_and_is_capped() {
        let from = Facing::from_radians(3.0);
        let to = Facing::from_radians(-3.0);
        let stepped = from.turned_towards(to, 0.1);
        assert!(stepped.radians() > 3.0, "{}", stepped.radians());
        assert!(from.delta_to(to).abs() < PI);
        assert_eq!(from.turned_towards(to, 1.0), to);
    }
}
