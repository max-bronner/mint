use super::*;

const ASIDE_SEARCH_CELLS: u32 = 1600;

#[derive(Clone, Copy)]
enum AsideClear<'a> {
    Route(&'a [Vec2]),
    Point(Vec2),
}

impl AsideClear<'_> {
    fn clears(&self, anchor: Vec2, half: f32, pushed: Vec2) -> bool {
        let inside = |p: Vec2| (p.x - anchor.x).abs() < half && (p.y - anchor.y).abs() < half;
        let pusher = match self {
            AsideClear::Route(route) => route.first().copied(),
            AsideClear::Point(p) => Some(*p),
        };
        if inside(pushed) || pusher.is_some_and(inside) {
            return false;
        }
        match self {
            AsideClear::Route(route) => {
                route.windows(2).all(|leg| match (leg.first(), leg.get(1)) {
                    (Some(a), Some(b)) => !segment_touches_box(*a, *b, anchor, half),
                    _ => true,
                })
            }
            AsideClear::Point(p) => {
                !((p.x - anchor.x).abs() <= half && (p.y - anchor.y).abs() <= half)
            }
        }
    }
}

fn segment_touches_box(a: Vec2, b: Vec2, centre: Vec2, half: f32) -> bool {
    let (mut near, mut far) = (0.0f32, 1.0f32);
    for (from, delta, lo, hi) in [
        (a.x, b.x - a.x, centre.x - half, centre.x + half),
        (a.y, b.y - a.y, centre.y - half, centre.y + half),
    ] {
        if delta == 0.0 {
            if from < lo || from > hi {
                return false;
            }
            continue;
        }
        let (mut t0, mut t1) = ((lo - from) / delta, (hi - from) / delta);
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        near = near.max(t0);
        far = far.min(t1);
        if near > far {
            return false;
        }
    }
    true
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
        let big = self
            .crushable_level(tick, ids, mover)
            .is_some_and(|level| level >= 4);
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
                || world
                    .get(ids.ignored_obstacle, mover)
                    .is_some_and(|ignored| ignored.0 == entity)
                || has(entity, "MOVE_FOR_NOONE")
                || (has(entity, "PATH_THROUGH_EACH_OTHER") && has(mover, "PATH_THROUGH_EACH_OTHER"))
                || self.holds_condition_in(world, entity, "THROWN_PROJECTILE")
                || self.carries_object_status(world, entity, "IS_LEAVING_FACTORY")
            {
                continue;
            }
            let moving = world.contains(ids.path, target);
            let attacking = self.charge_state(tick, ids, target).attacking;
            let goes = if through_infantry && !has(entity, "PATH_THROUGH_INFANTRY") {
                !(attacking || moving) || big
            } else if !self.has_ai(world, ids, entity) || moving {
                false
            } else {
                (self.regular_state_idle(world, ids, target)
                    && !Self::ai_busy(world, ids, target, tick.frame()))
                    || big
            };
            let goes = goes && !self.refuses_by_formation_status(world, ids, target);
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
        let pusher_cells = f32::from(self.path_footprint(tick, ids, mover).side());
        let pusher_radius = self.pusher_radius(tick.state(), ids, mover);
        let Some(destination) = route.last().copied() else {
            return Ok(());
        };
        for target in pushed {
            self.move_aside(
                tick,
                ids,
                pipeline,
                target,
                AsideClear::Route(route),
                (pusher_cells, pusher_radius),
                destination,
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
        let pusher_cells = f32::from(self.path_footprint(tick, ids, mover).side());
        let mover_radius = Self::mover_radius(tick, ids, mover);
        let pusher_radius = self.pusher_radius(tick.state(), ids, mover);
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
                || self.refuses_by_formation_status(world, ids, target)
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
            self.move_aside(
                tick,
                ids,
                &mut pipeline,
                target,
                avoid,
                (pusher_cells, pusher_radius),
                destination,
            )?;
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
        (pusher_cells, pusher_radius): (f32, f32),
        destination: Vec2,
    ) -> Result<()> {
        let now = tick.frame();
        if self.weapon_busy(tick.state(), ids, target, now)
            || Self::mid_portal(tick.state(), ids, target)
        {
            let (avoid, from_point) = match avoid {
                AsideClear::Route(route) => (route.to_vec(), false),
                AsideClear::Point(point) => (vec![point], true),
            };
            tick.state_mut().remove(ids.deferred_order, target);
            tick.state_mut().insert(
                ids.held_aside,
                target,
                super::busy::HeldAside {
                    avoid,
                    from_point,
                    radii: (pusher_cells, pusher_radius),
                    destination,
                },
            )?;
            return Ok(());
        }
        self.move_aside_now(
            tick,
            ids,
            pipeline,
            target,
            avoid,
            (pusher_cells, pusher_radius),
            destination,
        )
    }

    pub(super) fn replay_held_asides(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let held: Vec<Entity> = tick
            .state()
            .store(ids.held_aside)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for target in held {
            if self.weapon_busy(tick.state(), ids, target, now)
                || Self::mid_portal(tick.state(), ids, target)
            {
                continue;
            }
            let Some(slot) = tick.state_mut().remove(ids.held_aside, target) else {
                continue;
            };
            if !tick.state().is_live(target) || tick.state().contains(ids.doomed, target) {
                continue;
            }
            let avoid = if slot.from_point {
                match slot.avoid.first() {
                    Some(point) => AsideClear::Point(*point),
                    None => continue,
                }
            } else {
                AsideClear::Route(&slot.avoid)
            };
            let mut pipeline = Pipeline::new(self.modes(tick, ids).config());
            self.move_aside_now(
                tick,
                ids,
                &mut pipeline,
                target,
                avoid,
                slot.radii,
                slot.destination,
            )?;
            let points = tick
                .state()
                .get(ids.path, target)
                .map(|path| path.route.points.clone());
            if let Some(points) = points {
                self.move_allies(tick, ids, &mut pipeline, target, &points)?;
            }
        }
        Ok(())
    }

    fn move_aside_now(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        pipeline: &mut Pipeline,
        target: Entity,
        avoid: AsideClear<'_>,
        (pusher_cells, pusher_radius): (f32, f32),
        destination: Vec2,
    ) -> Result<()> {
        if self.move_refused(tick, ids, target).is_some() {
            return Ok(());
        }
        let Some(from) = tick.state().get(ids.body, target).map(|b| b.0.position) else {
            return Ok(());
        };
        let class = self
            .mover_class(tick, ids, target)
            .passing(self.portal_pass_aside(tick.state(), ids, target));
        let Some(start) = self.passable_cell_for(tick, ids, from, class) else {
            return Ok(());
        };
        let geometry = self.passability.geometry;
        let view = self.dynamic_view(tick, ids);
        let extent = view.extent();
        let footprint = self.path_footprint(tick, ids, target);
        let half = 5.0 * (f32::from(footprint.side()) + pusher_cells) - 2.5;
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
                let clear = avoid.clears(point, half, from);
                (clear && fits((x, y)) && self.reachable(class, start, cell)).then_some(point)
            });
        let Some(aside) = aside else {
            return Ok(());
        };
        let (_, end, points) = self.plan_via(
            tick,
            ids,
            pipeline,
            target,
            class,
            (start, from),
            &[aside],
            false,
        )?;
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
        let old = tick.state().get(ids.path, target).cloned();
        let for_good = self
            .pushed_goal(tick.state(), ids, target)
            .is_some_and(|goal| self.within_radius_3d(goal, destination, pusher_radius));
        self.note_order_origin(tick, ids, target)?;
        tick.state_mut().insert(
            ids.path,
            target,
            Path {
                route: Route {
                    points,
                    extra_distance: 0.0,
                    unsearched: false,
                    portals: Vec::new(),
                },
                goal: end,
                kind: PathKind::Ordered,
                facing: None,
            },
        )?;
        Self::start_move_aside_state(tick, ids, target, old, for_good)
    }

    pub(super) fn pusher_radius(&self, world: &World, ids: Ids, mover: Entity) -> f32 {
        crate::construction::object_bounding_radius(
            world,
            &self.kinds,
            (ids.kind, self.modules.as_ref().map(|wiring| wiring.state)),
            mover,
        )
    }

    pub(super) fn pushed_goal(&self, world: &World, ids: Ids, target: Entity) -> Option<Vec2> {
        world
            .get(ids.path, target)
            .map(|path| path.route.points.last().copied().unwrap_or(path.goal))
            .or_else(|| world.get(ids.order_goal, target).map(|goal| goal.0))
    }

    pub(super) fn within_radius_3d(&self, a: Vec2, b: Vec2, radius: f32) -> bool {
        let dz = self.ground_height(a.x, a.y) - self.ground_height(b.x, b.y);
        a.minus(b).length_squared() + dz * dz < radius * radius
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

impl RotwkRules {
    pub(super) fn refuses_by_formation_status(
        &self,
        world: &World,
        ids: Ids,
        target: Entity,
    ) -> bool {
        world.contains(ids.in_formation_template, target)
            && self.regular_state_idle(world, ids, target)
            && !(world.contains(ids.guard, target)
                || world
                    .get(ids.emotion, target)
                    .is_some_and(|emotion| emotion.state.is_some()))
    }
}
