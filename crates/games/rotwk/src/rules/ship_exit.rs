use super::*;
use mint_sage_passability as pass;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LastPathGoal(pub(crate) Vec2);

impl Component for LastPathGoal {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0.x);
        hasher.write_f32(self.0.y);
    }
}

const RECTANGLE_GROWTH: f32 = 20.0;
const INLAND: f32 = 50.0;

fn blocked_flags() -> mint_movement::CellFlags {
    [
        pass::CLIFF,
        pass::WATER,
        pass::DEEP_WATER,
        pass::OBSTACLE,
        pass::BOUNDARY,
        pass::RUBBLE,
        pass::IMPASSABLE_TO_PLAYERS,
    ]
    .into_iter()
    .fold(mint_movement::CellFlags::NONE, |all, flag| all.union(flag))
}

impl RotwkRules {
    pub(super) fn exits_by_landing(&self, world: &World, ids: Ids, container: Entity) -> bool {
        let _ = ids;
        self.contain_view(world, container)
            .is_some_and(|view| view.open.number_of_exit_paths() == 0)
    }

    pub(super) fn leave_by_landing(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        object: Entity,
        members: &[Entity],
    ) -> Result<()> {
        let Some(ship) = tick.state().get(ids.body, container).map(|body| body.0) else {
            return Ok(());
        };
        let landing = if self
            .record_of(tick, ids, container)
            .is_some_and(|record| record.has_kind("SHIP"))
        {
            self.ship_landing_point(tick, ids, container)
        } else {
            None
        };
        let at = landing.unwrap_or(ship.position);
        let away = at.minus(ship.position);
        let facing = Facing::from_vector(away);
        for entity in members {
            if let Some(mut body) = tick.state().get(ids.body, *entity).copied() {
                body.0.position = at;
                if let Some(facing) = facing {
                    body.0.facing = facing;
                }
                body.0.speed = 0.0;
                body.0.waypoint = 0;
                tick.state_mut().insert(ids.body, *entity, body)?;
            }
        }
        if landing.is_none() {
            return Ok(());
        }
        let points = self.ship_exit_route(tick, ids, (container, object), at, ship.position);
        if let Some(&goal) = points.last().filter(|_| points.len() > 1) {
            crate::unit::give_path(
                tick.state_mut(),
                (ids.body, ids.path),
                object,
                Path {
                    route: Route {
                        portals: vec![None; points.len()],
                        points,
                        extra_distance: 0.0,
                        unsearched: false,
                    },
                    goal,
                    kind: PathKind::Ordered,
                    facing: None,
                },
            )?;
            tick.state_mut()
                .insert(ids.ignored_obstacle, object, IgnoredObstacle(container))?;
        }
        Ok(())
    }

    pub(super) fn note_path_goals(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let goals: Vec<(Entity, Vec2)> = tick
            .state()
            .store(ids.path)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, path)| (entity, path.goal))
                    .collect()
            })
            .unwrap_or_default();
        for (entity, goal) in goals {
            if tick
                .state()
                .get(ids.last_path_goal, entity)
                .is_none_or(|last| last.0 != goal)
            {
                tick.state_mut()
                    .insert(ids.last_path_goal, entity, LastPathGoal(goal))?;
            }
        }
        Ok(())
    }

    pub(super) fn ship_landing_point(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        ship: Entity,
    ) -> Option<Vec2> {
        let body = tick.state().get(ids.body, ship)?.0;
        if !self.passability.underwater(body.position) {
            return Some(body.position);
        }
        let record = self.record_of(tick, ids, ship)?;
        let major = record.geometry.major_radius.unwrap_or(0.0);
        let is_box = record
            .geometry
            .shape
            .as_deref()
            .is_some_and(|shape| shape.eq_ignore_ascii_case("BOX"));
        let minor = if is_box {
            record.geometry.minor_radius.unwrap_or(0.0)
        } else {
            major
        };
        let geometry = self.passability.geometry;
        let cell = geometry.cell_size();
        let origin = geometry.origin();
        let extent = self.passability.grid.extent();
        #[allow(clippy::cast_possible_truncation)]
        let index = |v: f32, o: f32| ((v - o) / cell + 0.5).floor().clamp(-1.0e6, 1.0e6) as i32;
        let centre = (
            index(body.position.x, origin.x),
            index(body.position.y, origin.y),
        );
        #[allow(clippy::cast_possible_truncation)]
        let side = |radius: f32| ((radius + RECTANGLE_GROWTH) * 2.0 * 0.1).clamp(0.0, 1.0e4) as i32;
        let blocked = blocked_flags();
        let mut best: Option<(f32, Vec2)> = None;
        for (x, y) in super::cell_raster::rectangle_cells(
            centre,
            (side(major), side(minor)),
            body.facing.radians(),
        ) {
            let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                continue;
            };
            let candidate = mint_movement::Cell::new(x, y);
            if !extent.contains(candidate)
                || mint_movement::Passability::flags(&self.passability.grid, candidate)
                    .intersects(blocked)
            {
                continue;
            }
            let corner = Vec2::new(
                origin.x + f32::from(x) * cell,
                origin.y + f32::from(y) * cell,
            );
            let distance = corner.minus(body.position).length_squared();
            if best.is_none_or(|(least, _)| distance < least) {
                best = Some((distance, corner));
            }
        }
        best.map(|(_, at)| at)
    }

    pub(super) fn ship_exit_route(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        (ship, leaver): (Entity, Entity),
        from: Vec2,
        goal: Vec2,
    ) -> Vec<Vec2> {
        let ship_at = tick
            .state()
            .get(ids.body, ship)
            .map_or(from, |body| body.0.position);
        let p = tick
            .state()
            .get(ids.last_path_goal, ship)
            .map(|last| last.0)
            .filter(|last| last.minus(ship_at).length() >= INLAND)
            .unwrap_or_else(|| {
                let away = from.minus(ship_at);
                let length = away.length();
                let unit = if length > 0.0 {
                    away.scale(1.0 / length)
                } else {
                    Vec2::ZERO
                };
                from.plus(unit.scale(INLAND))
            });
        let cell = self.passability.geometry.cell_size();
        let mut points = vec![from];
        if p.minus(from).length() >= cell {
            points.push(p);
        }
        let last = points.last().copied().unwrap_or(from);
        let request = super::adjust_destination::Request {
            mover: leaver,
            dest: goal,
            group_goal: None,
        };
        let mut claims = super::adjust_destination::OrderClaims::none();
        if let Some(adjusted) = self.adjust_destination(tick, ids, request, &mut claims) {
            if adjusted.minus(last).length() >= cell {
                points.push(adjusted);
            }
        }
        points
    }
}
