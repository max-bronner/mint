use super::*;
use crate::wall_layer::Layer;

const SIGHT_ALLOWANCE: f32 = 0.5;

const LAYER_DROP: f32 = 20.0;

impl RotwkRules {
    pub(super) fn terrain_sight_height(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        shooter: Entity,
    ) -> Option<f32> {
        if !self.needs_line_of_sight(tick, ids, shooter)
            || !tick.state().contains(ids.armed, shooter)
            || self.has_kind_bit(tick.state(), ids, shooter, "IMMOBILE")
        {
            return None;
        }
        self.foe_of(tick, ids, shooter).map(|foe| foe.height)
    }

    pub(super) fn terrain_blocked(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        top: f32,
        from: (Vec2, f32),
        to: (Vec2, f32),
    ) -> bool {
        let heights = &self.passability.heights;
        let geometry = heights.geometry();
        let (size, origin) = (geometry.cell_size(), geometry.origin());
        let cell = |point: Vec2| {
            (
                ((point.x - origin.x) / size).floor(),
                ((point.y - origin.y) / size).floor(),
            )
        };
        let ((x0, y0), (x1, y1)) = (cell(from.0), cell(to.0));
        let (dx, dy) = (x1 - x0, y1 - y0);
        let cells = dx.abs().max(dy.abs());
        if cells.is_nan() || cells < 1.0 {
            return false;
        }
        let origin_z = from.1 + top;
        let rise = (to.1 - origin_z) / cells;
        let wall = crate::wall_layer::layer_of(tick.state(), ids.wall_layer);
        let extent = self.passability.grid.extent();
        let mut touched_ground = false;
        let mut step = 0.0f32;
        while step < cells {
            let share = step / cells;
            let (cx, cy) = ((x0 + dx * share).round(), (y0 + dy * share).round());
            let corner = |ox: f32, oy: f32| {
                heights.height_at(origin.x + (cx + ox) * size, origin.y + (cy + oy) * size)
            };
            let corners = [
                corner(0.0, 0.0),
                corner(1.0, 0.0),
                corner(0.0, 1.0),
                corner(1.0, 1.0),
            ];
            if corners.iter().any(Option::is_none) {
                return false;
            }
            let mut height = corners.iter().flatten().fold(f32::MIN, |a, b| a.max(*b));
            if let Some(wall) = wall {
                let centre = Vec2::new(origin.x + (cx + 0.5) * size, origin.y + (cy + 0.5) * size);
                if wall.layer_at(self.passability.geometry, extent, centre) == Layer::GROUND {
                    touched_ground = true;
                } else if touched_ground {
                    if let Some(surface) =
                        wall.surface_height(self.passability.geometry, extent, centre)
                    {
                        height = height.max(surface - LAYER_DROP);
                    }
                }
            }
            if height > origin_z + rise * step + SIGHT_ALLOWANCE {
                return true;
            }
            step += 1.0;
        }
        false
    }

    pub(super) fn terrain_blocks_fire(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        shooter: Entity,
        me: &Foe,
        victim: &Foe,
    ) -> bool {
        self.terrain_sight_height(tick, ids, shooter)
            .is_some_and(|top| {
                self.terrain_blocked(
                    tick,
                    ids,
                    top,
                    (me.position, me.z),
                    (victim.position, victim.z + victim.height * 0.5),
                )
            })
    }
}
