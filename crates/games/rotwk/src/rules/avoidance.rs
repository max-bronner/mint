use super::*;

pub(super) struct AvoidanceGrid {
    population: Vec<Neighbour>,
    entries: Vec<(i32, i32, u32)>,
}

impl AvoidanceGrid {
    pub(super) fn build(population: Vec<Neighbour>) -> Self {
        let entries: Vec<(i32, i32, u32)> = population
            .iter()
            .enumerate()
            .filter_map(|(at, neighbour)| {
                let index = u32::try_from(at).ok()?;
                Some((
                    grid_cell(neighbour.position.x),
                    grid_cell(neighbour.position.y),
                    index,
                ))
            })
            .collect();
        let mut entries = entries;
        entries.sort_unstable();
        Self {
            population,
            entries,
        }
    }
}

pub(super) struct IgnoringTarget<'a> {
    pub(super) grid: &'a AvoidanceGrid,
    pub(super) ignored: Option<MoverId>,
}

impl NeighbourQuery for IgnoringTarget<'_> {
    fn neighbours(&self, centre: Vec2, radius: f32, out: &mut Vec<Neighbour>) {
        let before = out.len();
        self.grid.neighbours(centre, radius, out);
        if let Some(ignored) = self.ignored {
            let mut at = before;
            while at < out.len() {
                if out.get(at).is_some_and(|n| n.id == ignored) {
                    out.remove(at);
                } else {
                    at = at.saturating_add(1);
                }
            }
        }
    }
}

impl NeighbourQuery for AvoidanceGrid {
    fn neighbours(&self, centre: Vec2, radius: f32, out: &mut Vec<Neighbour>) {
        let cx = grid_cell(centre.x);
        let cy = grid_cell(centre.y);
        let radius_cells = grid_radius_cells(radius);
        let before = out.len();
        let lo_cy = cy.saturating_sub(radius_cells);
        let hi_cy = cy.saturating_add(radius_cells);
        let hi_cx = cx.saturating_add(radius_cells);
        let mut x = cx.saturating_sub(radius_cells);
        while x <= hi_cx {
            let col_start = self.entries.partition_point(|&(ex, _, _)| ex < x);
            let col_end = self.entries.partition_point(|&(ex, _, _)| ex <= x);
            if let Some(column) = self.entries.get(col_start..col_end) {
                let row_start = column.partition_point(|&(_, ey, _)| ey < lo_cy);
                let row_end = column.partition_point(|&(_, ey, _)| ey <= hi_cy);
                if let Some(row) = column.get(row_start..row_end) {
                    for &(_, _, index) in row {
                        if let Some(neighbour) = self.population.get(index as usize) {
                            out.push(*neighbour);
                        }
                    }
                }
            }
            let Some(next) = x.checked_add(1) else {
                break;
            };
            x = next;
        }
        if let Some(appended) = out.get_mut(before..) {
            appended.sort_by_key(|neighbour| neighbour.id);
        }
    }
}

impl RotwkRules {
    fn never_an_obstacle(shape: &crate::unit::Shape) -> bool {
        use mint_sage_asset::KindOf;
        shape.kind_of.contains(KindOf::IMMOBILE)
    }

    pub(super) fn avoidance_population(tick: &Tick<'_>, ids: Ids) -> Vec<Neighbour> {
        let mut population = Vec::new();
        if let Some(store) = tick.state().store(Self::horde_ids(ids).horde) {
            for (entity, _) in store.iter() {
                let Some(body) = tick.state().get(ids.body, entity) else {
                    continue;
                };
                population.push(Neighbour {
                    id: MoverId(entity.index()),
                    position: body.0.position,
                    velocity: Vec2::ZERO,
                    radius: Self::mover_radius(tick, ids, entity),
                });
            }
        }
        if let Some(store) = tick.state().store(ids.shape) {
            for (entity, shape) in store.iter() {
                if tick.state().contains(ids.member, entity) {
                    continue;
                }
                if Self::never_an_obstacle(shape) {
                    continue;
                }
                let Some(body) = tick.state().get(ids.body, entity) else {
                    continue;
                };
                population.push(Neighbour {
                    id: MoverId(entity.index()),
                    position: body.0.position,
                    velocity: Vec2::ZERO,
                    radius: shape.radius,
                });
            }
        }
        population.sort_by_key(|neighbour| neighbour.id);
        population
    }
}

#[cfg(test)]
mod tests {
    use mint_sage_asset::KindOf;

    use super::RotwkRules;
    use crate::unit::Shape;

    fn shape(kind_of: KindOf, geometry_is_small: bool) -> Shape {
        Shape {
            kind_of,
            radius: 5.0,
            height: 18.0,
            geometry_is_small,
        }
    }

    #[test]
    fn only_an_immobile_shape_is_never_an_obstacle() {
        let rows = [
            (
                shape(KindOf::IMMOBILE, false),
                true,
                "a non-structure, non-small, collidable prop: still no push in retail",
            ),
            (
                shape(KindOf::IMMOBILE.union(KindOf::STRUCTURE), false),
                true,
                "a structure: the static grid already stamps it",
            ),
            (
                shape(KindOf::IMMOBILE, true),
                true,
                "GeometryIsSmall: a crate",
            ),
            (
                shape(KindOf::IMMOBILE.union(KindOf::NO_COLLIDE), false),
                true,
                "NO_COLLIDE",
            ),
            (shape(KindOf::NONE, true), false, "mobile and small"),
            (
                shape(KindOf::NONE.union(KindOf::NO_COLLIDE), false),
                false,
                "mobile and no-collide",
            ),
        ];
        for (shape, never_an_obstacle, label) in rows {
            assert_eq!(
                RotwkRules::never_an_obstacle(&shape),
                never_an_obstacle,
                "{label}"
            );
        }
    }
}
