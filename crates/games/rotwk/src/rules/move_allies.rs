use super::*;

const ASIDE_SEARCH_CELLS: u32 = 1600;

#[derive(Clone, Copy)]
enum AsideClear<'a> {
    Route(&'a [Vec2]),
    Point(Vec2),
}

impl AsideClear<'_> {
    fn clears(&self, point: Vec2, clearance: f32) -> bool {
        match self {
            AsideClear::Route(route) => {
                route.windows(2).all(|leg| match (leg.first(), leg.get(1)) {
                    (Some(a), Some(b)) => distance_to_segment(point, *a, *b) > clearance,
                    _ => true,
                })
            }
            AsideClear::Point(p) => point.minus(*p).length() > clearance,
        }
    }
}

impl RotwkRules {
    pub(super) fn move_allies(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        pipeline: &mut Pipeline,
        mover: Entity,
        route: &[Vec2],
    ) -> Result<()> {
        let world = tick.state();
        let has = |entity: Entity, bit: &str| self.has_kind_bit(world, ids, entity, bit);
        if route.len() < 2 || has(mover, "SUBMARINE") {
            return Ok(());
        }
        let Some(owner) = self.owner_of(tick, ids, mover) else {
            return Ok(());
        };
        let horde_mover = world.contains(ids.horde, mover);
        let own_horde = world.get(ids.member, mover).map(|member| member.horde);
        let through_infantry = has(mover, "PATH_THROUGH_INFANTRY");
        let crush = self
            .record_of(tick, ids, mover)
            .map(|record| record.crush.clone());
        let force = crush.as_ref().is_some_and(|crush| crush.crusher_level >= 4);
        let geometry = self.passability.geometry;
        let line = swept_cells(geometry, route, 1);
        let corridor = swept_cells(geometry, route, self.sweep_cells(tick, ids, mover));

        let sight = self.body_sight(world, ids, mover);
        let walking = |entity: Entity| {
            world.contains(ids.path, entity)
                || world
                    .get(ids.member, entity)
                    .is_some_and(|m| m.horde != mover && world.contains(ids.path, m.horde))
        };
        let own_members: Vec<Entity> = world
            .get(ids.horde, mover)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        let candidates = self.ally_candidates(tick, ids, mover, owner);
        let extent = self.passability.grid.extent();
        let blocked = own_members
            .iter()
            .filter_map(|&member| Some((member, world.get(ids.body, member)?.0.position)))
            .chain(candidates.iter().copied())
            .any(|(entity, at)| {
                geometry
                    .cell_at(at, 0, extent)
                    .is_some_and(|cell| line.contains(&(cell.x, cell.y)))
                    && !walking(entity)
                    && !self.sees_past(world, ids, sight, entity, true)
            });
        let mut pushed: Vec<Entity> = Vec::new();
        for (entity, at) in candidates {
            let Some(cell) = geometry.cell_at(at, 0, extent) else {
                continue;
            };
            if !corridor.contains(&(cell.x, cell.y)) {
                continue;
            }
            let target = world.get(ids.member, entity).map_or(entity, |m| m.horde);
            if pushed.contains(&target)
                || target == mover
                || Some(target) == own_horde
                || (horde_mover && world.contains(ids.horde, target))
                || has(entity, "MOVE_FOR_NOONE")
                || (has(entity, "PATH_THROUGH_EACH_OTHER") && has(mover, "PATH_THROUGH_EACH_OTHER"))
                || world.contains(ids.ignored_obstacle, target)
            {
                continue;
            }
            let target_moving = world.contains(ids.path, target);
            let attacking = self.charge_state(tick, ids, target).attacking;
            let goes = if through_infantry && !has(entity, "PATH_THROUGH_INFANTRY") {
                (!attacking && !target_moving) || force
            } else {
                !target_moving && (!attacking || force)
            };
            let goes = goes && !world.contains(ids.in_formation_template, target);
            if goes {
                pushed.push(target);
            }
        }
        let blocked = blocked
            && !has(mover, "NO_COLLIDE")
            && (!horde_mover || self.group_formation.hordes_wait_for_hordes);
        let wants = blocked || crush.is_some_and(|crush| crush.allies) || through_infantry;
        let pusher = through_infantry || has(mover, "DOZER") || has(mover, "HARVESTER");
        if !wants || !(pusher || blocked) {
            return Ok(());
        }
        let mover_radius = Self::mover_radius(tick, ids, mover);
        for target in pushed {
            self.move_aside(
                tick,
                ids,
                pipeline,
                target,
                AsideClear::Route(route),
                mover_radius,
            )?;
        }
        Ok(())
    }

    pub(super) fn move_allies_from_destination(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        mover: Entity,
        own_horde: Option<Entity>,
        from: Vec2,
        route: &[Vec2],
        has_path: bool,
    ) -> Result<()> {
        let Some(destination) = route.last().copied() else {
            return Ok(());
        };
        let Some(owner) = self.owner_of(tick, ids, mover) else {
            return Ok(());
        };
        let geometry = self.passability.geometry;
        let line = swept_cells(geometry, &[from, destination], 1);
        let mover_radius = Self::mover_radius(tick, ids, mover);
        let world = tick.state();
        let mut pushed: Vec<Entity> = Vec::new();
        for (entity, at) in self.ally_candidates(tick, ids, mover, owner) {
            let Some(cell) = geometry.cell_at(at, 0, self.passability.grid.extent()) else {
                continue;
            };
            if !line.contains(&(cell.x, cell.y)) {
                continue;
            }
            let target = world.get(ids.member, entity).map_or(entity, |m| m.horde);
            let elsewhere = world
                .get(ids.path, target)
                .is_some_and(|path| path.goal.minus(destination).length() >= mover_radius);
            if pushed.contains(&target)
                || Some(target) == own_horde
                || elsewhere
                || world.contains(ids.in_formation_template, target)
            {
                continue;
            }
            pushed.push(target);
        }
        if pushed.is_empty() {
            return Ok(());
        }
        let mut pipeline = Pipeline::new(self.modes(tick, ids).config());
        let mut walk = Vec::with_capacity(route.len().saturating_add(1));
        walk.push(from);
        walk.extend_from_slice(route);
        let avoid = if has_path {
            AsideClear::Route(&walk)
        } else {
            AsideClear::Point(from)
        };
        for target in pushed {
            self.move_aside(tick, ids, &mut pipeline, target, avoid, mover_radius)?;
        }
        Ok(())
    }

    fn move_aside(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        pipeline: &mut Pipeline,
        target: Entity,
        avoid: AsideClear<'_>,
        mover_radius: f32,
    ) -> Result<()> {
        if self.move_refused(tick, ids, target).is_some() {
            return Ok(());
        }
        let Some(from) = tick.state().get(ids.body, target).map(|b| b.0.position) else {
            return Ok(());
        };
        let class = self.mover_class(tick, ids, target);
        let Some(start) = self.passable_cell_for(tick, ids, from, class) else {
            return Ok(());
        };
        let clearance = mover_radius + Self::mover_radius(tick, ids, target);
        let geometry = self.passability.geometry;
        let view = self.dynamic_view(tick, ids);
        let extent = view.extent();
        let footprint = self.path_footprint(tick, ids, target);
        #[allow(clippy::cast_precision_loss)]
        let reach = ((ASIDE_SEARCH_CELLS as f32).sqrt() + f32::from(footprint.side()) + 1.0)
            * geometry.cell_size();
        let units = self.unit_cells(tick, ids, target, None, from, reach);
        let fits = |at: (i32, i32)| {
            let Some(rect) = crate::goal_cells::rect(extent, at, footprint) else {
                return false;
            };
            let (x0, y0, x1, y1) = rect.span();
            let covered = || (y0..y1).flat_map(move |y| (x0..x1).map(move |x| (x, y)));
            covered().all(|(x, y)| match (u16::try_from(x), u16::try_from(y)) {
                (Ok(x), Ok(y)) => view.is_passable(Cell::new(x, y), class),
                _ => false,
            }) && units.destination_clear(covered())
        };
        let aside = super::attack::melee_search_offsets(ASIDE_SEARCH_CELLS)
            .iter()
            .filter(|&&(dx, dy)| (dx, dy) != (0, 0))
            .find_map(|&(dx, dy)| {
                let x = i32::from(start.x).checked_add(dx)?;
                let y = i32::from(start.y).checked_add(dy)?;
                let cell = Cell::new(u16::try_from(x).ok()?, u16::try_from(y).ok()?);
                if !extent.contains(cell) {
                    return None;
                }
                let point = crate::goal_cells::anchor_point(geometry, (x, y), footprint);
                let clear = avoid.clears(point, clearance);
                (clear && fits((x, y)) && self.reachable(class, start, cell)).then_some(point)
            });
        let Some(aside) = aside else {
            return Ok(());
        };
        let (_, end, points) =
            self.plan_via(tick, ids, pipeline, target, class, (start, from), &[aside])?;
        if points.len() < 2 {
            return Ok(());
        }
        crate::diag::diag!(
            "allies: entity {} steps aside to ({:.0}, {:.0})",
            target.index(),
            end.x,
            end.y
        );
        if let Some(body) = tick.state().get(ids.body, target).copied() {
            let mut state = body.0;
            state.waypoint = 0;
            tick.state_mut().insert(ids.body, target, Body(state))?;
        }
        tick.state_mut().insert(
            ids.path,
            target,
            Path {
                route: Route {
                    points,
                    extra_distance: 0.0,
                    portals: Vec::new(),
                },
                goal: end,
                kind: PathKind::Ordered,
                facing: None,
            },
        )?;
        Ok(())
    }

    fn sweep_cells(&self, tick: &Tick<'_>, ids: Ids, mover: Entity) -> u32 {
        let world = tick.state();
        let big = ["MONSTER", "HORDE", "SHIP"]
            .iter()
            .any(|bit| self.has_kind_bit(world, ids, mover, bit))
            || world.contains(ids.horde, mover);
        let base: u32 = if big { 4 } else { 2 };
        let mut diameter = 2.0 * Self::mover_radius(tick, ids, mover);
        if diameter > 10.0 && diameter < 20.0 {
            diameter = 20.0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cells =
            (diameter / self.passability.geometry.cell_size() + 0.3).clamp(1.0, 64.0) as u32;
        cells.min(base.saturating_mul(2))
    }

    fn ally_candidates(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        mover: Entity,
        owner: PlayerId,
    ) -> Vec<(Entity, Vec2)> {
        let world = tick.state();
        let mover_members: Vec<Entity> = world
            .get(ids.horde, mover)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        world
            .store(ids.body)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, _)| *entity != mover && !mover_members.contains(entity))
                    .filter(|(entity, _)| !world.contains(ids.horde, *entity))
                    .filter(|(entity, _)| {
                        world.get(ids.owner, *entity).is_some_and(|o| {
                            o.0 == owner || self.object_allied(world, mover, *entity)
                        })
                    })
                    .filter(|(entity, _)| !world.contains(ids.doomed, *entity))
                    .map(|(entity, body)| (entity, body.0.position))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn route_crosses_structure(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        structure: Entity,
        route: &[Vec2],
    ) -> bool {
        let building = self.footprint_cells_of(tick.state(), ids, structure);
        if building.is_empty() {
            return false;
        }
        let line = swept_cells(self.passability.geometry, route, 1);
        building.iter().any(|cell| line.contains(&(cell.x, cell.y)))
    }

    pub(super) fn owner_of(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<PlayerId> {
        let world = tick.state();
        world.get(ids.owner, entity).map(|o| o.0).or_else(|| {
            world
                .get(ids.horde, entity)?
                .members
                .iter()
                .find_map(|m| world.get(ids.owner, *m).map(|o| o.0))
        })
    }
}

pub(super) fn swept_cells(
    geometry: mint_movement::CellGeometry,
    route: &[Vec2],
    width: u32,
) -> BTreeSet<(u16, u16)> {
    let size = geometry.cell_size();
    let origin = geometry.origin();
    #[allow(clippy::cast_possible_truncation)]
    let index = |v: f32, o: f32| ((v - o) / size).floor().clamp(-1.0e6, 1.0e6) as i64;
    let low = i64::from(width / 2);
    let high = i64::from(width) - low;
    let mut cells = BTreeSet::new();
    for leg in route.windows(2) {
        let (Some(a), Some(b)) = (leg.first(), leg.get(1)) else {
            continue;
        };
        let (ax, ay) = (index(a.x, origin.x), index(a.y, origin.y));
        let (bx, by) = (index(b.x, origin.x), index(b.y, origin.y));
        let steps = (bx - ax).abs().max((by - ay).abs()).clamp(0, 100_000);
        for step in 0..=steps {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let (cx, cy) = if steps == 0 {
                (ax, ay)
            } else {
                let t = step as f64 / steps as f64;
                (
                    ax + ((bx - ax) as f64 * t).round() as i64,
                    ay + ((by - ay) as f64 * t).round() as i64,
                )
            };
            for y in (cy - low)..(cy + high) {
                for x in (cx - low)..(cx + high) {
                    if let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) {
                        cells.insert((x, y));
                    }
                }
            }
        }
    }
    cells
}

fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b.minus(a);
    let squared = ab.length_squared();
    let t = if squared > 0.0 {
        (p.minus(a).dot(ab) / squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.minus(a.plus(ab.scale(t))).length()
}
