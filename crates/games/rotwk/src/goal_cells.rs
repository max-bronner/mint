use mint_movement::{CellGeometry, GridExtent, Vec2};
use mint_sim::{Component, ComponentId, Entity, StateHasher, Tick, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Footprint {
    half: u16,
    centred: bool,
}

impl Footprint {
    pub(crate) const THREE_CENTRED: Self = Self {
        half: 1,
        centred: true,
    };

    pub(crate) const ONE_CENTRED: Self = Self {
        half: 0,
        centred: true,
    };

    pub(crate) const RADIUS_ONE: Self = Self {
        half: 1,
        centred: false,
    };

    pub(crate) const EIGHT: Self = Self {
        half: 4,
        centred: false,
    };

    pub(crate) fn of(
        radius: f32,
        large: bool,
        cell_size: f32,
        pathfind_diameter: Option<f32>,
    ) -> Self {
        let cap: u16 = if large { 4 } else { 2 };
        let diameter = match pathfind_diameter {
            Some(overridden) if overridden > 0.0 => overridden,
            _ => {
                let widened = 2.0 * radius;
                if widened > cell_size && widened < 2.0 * cell_size {
                    2.0 * cell_size
                } else {
                    widened
                }
            }
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cells = (diameter / cell_size + 0.3).floor().clamp(0.0, 1000.0) as u16;
        let cells = cells.max(1);
        let half = cells / 2;
        if half > cap {
            Self {
                half: cap,
                centred: true,
            }
        } else {
            Self {
                half,
                centred: cells % 2 == 1,
            }
        }
    }

    pub(crate) fn side(self) -> u16 {
        self.half
            .saturating_mul(2)
            .saturating_add(u16::from(self.centred))
    }
}

pub(crate) fn footprint_of(
    world: &World,
    ids: crate::horde::HordeIds,
    kinds: &crate::templates::KindTable,
    entity: Entity,
    geometry: CellGeometry,
) -> Footprint {
    let radius = world
        .get(ids.shape, entity)
        .map_or(0.0, |shape| shape.radius);
    let entry = world
        .get(ids.kind, entity)
        .and_then(|kind| kinds.get(*kind));
    let large = entry.is_some_and(|entry| entry.facts.large_footprint);
    let pathfind_diameter = entry.and_then(|entry| entry.record.pathfind_diameter);
    Footprint::of(radius, large, geometry.cell_size(), pathfind_diameter)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellRect {
    x0: u16,
    y0: u16,
    side: u16,
}

impl CellRect {
    pub(crate) fn overlaps(self, other: Self) -> bool {
        self.reaches(other, 0)
    }

    pub(crate) fn touches(self, other: Self) -> bool {
        self.reaches(other, 1)
    }

    fn bounds(self, by: u32) -> (u32, u32, u32, u32) {
        let side = u32::from(self.side);
        let x = u32::from(self.x0);
        let y = u32::from(self.y0);
        (
            x.saturating_sub(by),
            y.saturating_sub(by),
            x.saturating_add(side).saturating_add(by),
            y.saturating_add(side).saturating_add(by),
        )
    }

    fn reaches(self, other: Self, by: u32) -> bool {
        let (lx, ly, hx, hy) = self.bounds(by);
        let (ox, oy, ohx, ohy) = other.bounds(0);
        lx < ohx && ox < hx && ly < ohy && oy < hy
    }

    pub(crate) fn span(self) -> (i32, i32, i32, i32) {
        let side = i32::from(self.side);
        let (x0, y0) = (i32::from(self.x0), i32::from(self.y0));
        (x0, y0, x0 + side, y0 + side)
    }

    pub(crate) fn world(self, geometry: CellGeometry) -> (Vec2, f32) {
        let size = geometry.cell_size();
        let half = f32::from(self.side) * size * 0.5;
        let origin = geometry.origin();
        (
            Vec2::new(
                origin.x + f32::from(self.x0) * size + half,
                origin.y + f32::from(self.y0) * size + half,
            ),
            half,
        )
    }

    fn contains(self, (x, y): (u16, u16)) -> bool {
        let (x0, y0, hx, hy) = self.bounds(0);
        (x0..hx).contains(&u32::from(x)) && (y0..hy).contains(&u32::from(y))
    }

    fn cells(self) -> impl Iterator<Item = (u16, u16)> {
        let (x0, y0, side) = (self.x0, self.y0, self.side);
        (0..side).flat_map(move |dy| {
            (0..side).filter_map(move |dx| Some((x0.checked_add(dx)?, y0.checked_add(dy)?)))
        })
    }
}

pub(crate) fn anchor(geometry: CellGeometry, point: Vec2, footprint: Footprint) -> (i32, i32) {
    let size = geometry.cell_size();
    let fx = (point.x - geometry.origin().x) / size;
    let fy = (point.y - geometry.origin().y) / size;
    let (fx, fy) = if footprint.centred {
        (fx.floor(), fy.floor())
    } else {
        ((fx + 0.5).floor(), (fy + 0.5).floor())
    };
    #[allow(clippy::cast_possible_truncation)]
    let cell = (
        fx.clamp(-1.0e6, 1.0e6) as i32,
        fy.clamp(-1.0e6, 1.0e6) as i32,
    );
    cell
}

pub(crate) fn anchor_point(geometry: CellGeometry, cell: (i32, i32), footprint: Footprint) -> Vec2 {
    let size = geometry.cell_size();
    let half = if footprint.centred { 0.5 } else { 0.0 };
    #[allow(clippy::cast_precision_loss)]
    let at = Vec2::new(
        geometry.origin().x + (cell.0 as f32 + half) * size,
        geometry.origin().y + (cell.1 as f32 + half) * size,
    );
    at
}

pub(crate) fn answer_point(geometry: CellGeometry, cell: (i32, i32), footprint: Footprint) -> Vec2 {
    let size = geometry.cell_size();
    let offset = if footprint.centred { 0.5 } else { 0.05 };
    #[allow(clippy::cast_precision_loss)]
    let at = Vec2::new(
        geometry.origin().x + (cell.0 as f32 + offset) * size,
        geometry.origin().y + (cell.1 as f32 + offset) * size,
    );
    at
}

pub(crate) fn within_gap(
    at_a: (i32, i32),
    a: Footprint,
    at_b: (i32, i32),
    b: Footprint,
    gap: i32,
) -> bool {
    let (ax0, ay0, ax1, ay1) = span(at_a, a);
    let (bx0, by0, bx1, by1) = span(at_b, b);
    ax0 - gap <= bx1 && bx0 <= ax1 + gap && ay0 - gap <= by1 && by0 <= ay1 + gap
}

pub(crate) fn rings_hold(
    at: (i32, i32),
    f: Footprint,
    rings: i32,
    cells: &[mint_movement::Cell],
) -> bool {
    let (x0, y0, x1, y1) = span(at, f);
    cells.iter().any(|cell| {
        let (x, y) = (i32::from(cell.x), i32::from(cell.y));
        let grown = x0 - rings <= x && x < x1 + rings && y0 - rings <= y && y < y1 + rings;
        let own = x0 <= x && x < x1 && y0 <= y && y < y1;
        grown && !own
    })
}

fn span(at: (i32, i32), f: Footprint) -> (i32, i32, i32, i32) {
    let half = i32::from(f.half);
    let side = i32::from(f.side());
    (
        at.0 - half,
        at.1 - half,
        at.0 - half + side,
        at.1 - half + side,
    )
}

pub(crate) fn rect(extent: GridExtent, cell: (i32, i32), footprint: Footprint) -> Option<CellRect> {
    let half = i32::from(footprint.half);
    let x0 = u16::try_from(cell.0.checked_sub(half)?).ok()?;
    let y0 = u16::try_from(cell.1.checked_sub(half)?).ok()?;
    let side = footprint.side();
    let end = |start: u16| u32::from(start) + u32::from(side);
    (end(x0) <= u32::from(extent.width) && end(y0) <= u32::from(extent.height))
        .then_some(CellRect { x0, y0, side })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Claim {
    entity: Entity,
    rect: CellRect,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GoalCells {
    counts: Vec<u16>,
    width: u16,
    claims: Vec<Claim>,
}

impl GoalCells {
    pub(crate) fn empty(extent: GridExtent) -> Self {
        let cells = usize::from(extent.width).saturating_mul(usize::from(extent.height));
        Self {
            counts: vec![0; cells],
            width: extent.width,
            claims: Vec::new(),
        }
    }

    fn slot(&self, (x, y): (u16, u16)) -> usize {
        usize::from(y)
            .saturating_mul(usize::from(self.width))
            .saturating_add(usize::from(x))
    }

    fn others(&self, entity: Entity, rect: CellRect) -> impl Iterator<Item = u16> + '_ {
        let own = self.held(entity);
        rect.cells().map(move |cell| {
            let count = self.counts.get(self.slot(cell)).copied().unwrap_or(0);
            let mine = own.is_some_and(|own| own.contains(cell));
            count.saturating_sub(u16::from(mine))
        })
    }

    pub(crate) fn free_for(&self, entity: Entity, rect: CellRect) -> bool {
        self.others(entity, rect).all(|count| count == 0)
    }

    pub(crate) fn crowding(&self, entity: Entity, rect: CellRect) -> u32 {
        self.others(entity, rect)
            .fold(0u32, |sum, count| sum.saturating_add(u32::from(count)))
    }

    pub(crate) fn claimants_of(&self, entity: Entity, rect: CellRect) -> Vec<Entity> {
        if self.free_for(entity, rect) {
            return Vec::new();
        }
        self.claims
            .iter()
            .filter(|claim| claim.entity != entity && claim.rect.overlaps(rect))
            .map(|claim| claim.entity)
            .collect()
    }

    pub(crate) fn claimants(&self, entity: Entity, rect: CellRect) -> usize {
        self.claimants_of(entity, rect).len()
    }

    pub(crate) fn held(&self, entity: Entity) -> Option<CellRect> {
        self.find(entity)
            .ok()
            .and_then(|at| self.claims.get(at))
            .map(|claim| claim.rect)
    }

    fn find(&self, entity: Entity) -> Result<usize, usize> {
        self.claims
            .binary_search_by_key(&entity.index(), |claim| claim.entity.index())
    }

    pub(crate) fn claim(&mut self, entity: Entity, rect: CellRect) {
        if self.held(entity) == Some(rect) {
            return;
        }
        self.release(entity);
        for cell in rect.cells() {
            let slot = self.slot(cell);
            if let Some(count) = self.counts.get_mut(slot) {
                *count = count.saturating_add(1);
            }
        }
        if let Err(at) = self.find(entity) {
            self.claims.insert(at, Claim { entity, rect });
        }
    }

    pub(crate) fn release(&mut self, entity: Entity) {
        let Ok(at) = self.find(entity) else {
            return;
        };
        let claim = self.claims.remove(at);
        for cell in claim.rect.cells() {
            let slot = self.slot(cell);
            if let Some(count) = self.counts.get_mut(slot) {
                *count = count.saturating_sub(1);
            }
        }
    }

    fn dead_claims(
        &self,
        world: &World,
        health: ComponentId<crate::combat::Health>,
    ) -> Vec<Entity> {
        self.claims
            .iter()
            .map(|claim| claim.entity)
            .filter(|entity| {
                !world.is_live(*entity)
                    || world
                        .get(health, *entity)
                        .is_some_and(|health| health.0.is_dead())
            })
            .collect()
    }
}

impl Component for GoalCells {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.claims.len());
        for claim in &self.claims {
            claim.entity.hash_into(hasher);
            hasher.write_u16(claim.rect.x0);
            hasher.write_u16(claim.rect.y0);
            hasher.write_u16(claim.rect.side);
        }
    }
}

pub(crate) fn layer_mut<'a>(
    tick: &'a mut Tick<'_>,
    id: ComponentId<GoalCells>,
) -> Option<&'a mut GoalCells> {
    let clock = tick.state().store(id)?.entities().first().copied()?;
    tick.state_mut().get_mut(id, clock)
}

pub(crate) fn layer(world: &World, id: ComponentId<GoalCells>) -> Option<&GoalCells> {
    let clock = world.store(id)?.entities().first().copied()?;
    world.get(id, clock)
}

pub(crate) fn reap_dead_claims(
    tick: &mut Tick<'_>,
    id: ComponentId<GoalCells>,
    health: ComponentId<crate::combat::Health>,
) {
    let dead = layer(tick.state(), id)
        .map(|layer| layer.dead_claims(tick.state(), health))
        .unwrap_or_default();
    if dead.is_empty() {
        return;
    }
    if let Some(layer) = layer_mut(tick, id) {
        for entity in dead {
            layer.release(entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_footprint_follows_get_radius_and_center() {
        for (radius, large, side, centred) in [
            (8.0, false, 2, false),
            (3.0, false, 1, true),
            (12.0, false, 2, false),
            (16.0, false, 3, true),
            (54.0, false, 5, true),
            (54.0, true, 9, true),
        ] {
            let footprint = Footprint::of(radius, large, 10.0, None);
            assert_eq!(
                (footprint.side(), footprint.centred),
                (side, centred),
                "radius {radius} large {large}"
            );
        }
    }

    #[test]
    fn pathfind_diameter_overrides_the_footprint() {
        let overridden = Footprint::of(8.0, false, 10.0, Some(40.0));
        assert_eq!((overridden.side(), overridden.centred), (4, false));
        let plain = Footprint::of(8.0, false, 10.0, None);
        assert_eq!((plain.side(), plain.centred), (2, false));
    }

    #[test]
    fn a_large_footprint_is_not_cut_to_five_cells() {
        assert_eq!(Footprint::of(40.0, true, 10.0, None).side(), 8);
        assert_eq!(Footprint::of(40.0, false, 10.0, None).side(), 5);
    }

    #[test]
    fn a_structure_cell_is_in_reach_within_the_rings_but_not_under_the_footprint() {
        let one = Footprint::of(3.0, false, 10.0, None);
        let holds = |x, y| rings_hold((5, 5), one, 2, &[mint_movement::Cell::new(x, y)]);
        assert!(holds(7, 5), "the second ring");
        assert!(holds(3, 3), "the second ring's corner");
        assert!(!holds(8, 5), "past the rings");
        assert!(!holds(5, 5), "the footprint's own cell");
    }

    #[test]
    fn a_claimed_cell_refuses_another_unit_until_released() {
        let extent = GridExtent::new(20, 20).unwrap();
        let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
        let footprint = Footprint::of(8.0, false, 10.0, None);
        let at = |x, y| {
            rect(
                extent,
                anchor(geometry, Vec2::new(x, y), footprint),
                footprint,
            )
            .unwrap()
        };
        let mut cells = GoalCells::empty(extent);
        let (a, b) = {
            let mut world = World::new();
            (world.spawn().unwrap(), world.spawn().unwrap())
        };
        cells.claim(a, at(50.0, 50.0));
        assert!(cells.free_for(a, at(50.0, 50.0)));
        assert!(
            !cells.free_for(b, at(56.0, 50.0)),
            "one shared cell refuses"
        );
        assert!(cells.free_for(b, at(70.0, 50.0)), "disjoint squares do not");
        cells.release(a);
        assert!(cells.free_for(b, at(56.0, 50.0)));
    }
}
