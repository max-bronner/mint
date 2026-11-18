use super::attack::{Claimant, CLAIM_BUCKET};
use super::unit_cells::{DestinationAsker, GoalVerdict, UnitCells};
use super::*;
use crate::goal_cells::{self, Footprint};
use mint_movement::destination::{Claims, Footprint as Body};
use mint_sage_passability as pass;

const MAX_HEIGHT_STEP: f32 = 50.0;
const SPIRAL_ROUNDS: i32 = 40;
const LARGE_RECTANGLE_SIDE_BIAS: f32 = 4.0;
const CELLS_PER_UNIT: f32 = 0.1;
const SECTORS: f32 = 12.0;

#[derive(Debug, Clone, Copy)]
pub(super) struct Request {
    pub(super) mover: Entity,
    pub(super) dest: Vec2,
    pub(super) group_goal: Option<Vec2>,
}

pub(super) struct OrderClaims {
    others: Vec<Claimant>,
    taken: BTreeSet<(i32, i32)>,
    reserved: Vec<Body>,
    bodies: Option<(DestinationAsker, Claims)>,
}

impl OrderClaims {
    pub(super) fn none() -> Self {
        Self {
            others: Vec::new(),
            taken: BTreeSet::new(),
            reserved: Vec::new(),
            bodies: None,
        }
    }

    pub(super) fn for_order(
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        formations_on: bool,
        ordered: &[Entity],
    ) -> Self {
        let ordered: BTreeSet<Entity> = ordered.iter().copied().collect();
        let mut others = rules.claimants(tick, ids, formations_on);
        others.retain(|claimant| !ordered.contains(&claimant.entity));
        Self {
            others,
            ..Self::none()
        }
    }

    pub(super) fn claim(
        &mut self,
        cells: impl IntoIterator<Item = (i32, i32)>,
        body: Option<Body>,
    ) {
        self.taken.extend(cells);
        if let Some(body) = body {
            self.reserved.push(body);
            if let Some((_, claims)) = &mut self.bodies {
                claims.insert(body);
            }
        }
    }

    fn prepare(&mut self, rules: &RotwkRules, asker: DestinationAsker) {
        if self.bodies.as_ref().is_none_or(|(held, _)| *held != asker) {
            let mut claims = Claims::new(CLAIM_BUCKET);
            for claimant in &self.others {
                for (_, member_body, holder) in &claimant.goals {
                    if rules.goal_verdict(asker, *holder) != GoalVerdict::Clear {
                        claims.insert(*member_body);
                    }
                }
            }
            for body in &self.reserved {
                claims.insert(*body);
            }
            self.bodies = Some((asker, claims));
        }
    }
}

pub(super) fn quantise_to_twelfths(radians: f32) -> f32 {
    let turn = core::f32::consts::TAU;
    let wrapped = radians.rem_euclid(turn);
    let twelfths = wrapped * SECTORS / turn;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let sector = if twelfths > 11.0 {
        0.0
    } else {
        (twelfths + 0.5) as i32 as f32
    };
    sector * turn / SECTORS
}

struct Probe<'a> {
    rules: &'a RotwkRules,
    ids: Ids,
    world: &'a World,
    class: mint_movement::MoverClass,
    confined: mint_movement::MoverClass,
    view: mint_sage_passability::GatedView<'a>,
    extent: mint_movement::GridExtent,
    start: Option<Cell>,
    aircraft: bool,
    dozer: bool,
    footprint: Footprint,
    rectangle: Option<(i32, i32)>,
    heading: f32,
    layer_tag: u8,
    on_ground: bool,
    horde: bool,
    goal: Option<Cell>,
    claims: UnitCells,
    body: Option<(Body, Vec2)>,
    asker: DestinationAsker,
}

impl RotwkRules {
    pub(super) fn adjust_destination(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        request: Request,
        order: &mut OrderClaims,
    ) -> Option<Vec2> {
        let world = tick.state();
        let mover = request.mover;
        let record = self.record_of(tick, ids, mover);
        if record.is_some_and(|record| record.has_kind("PROJECTILE")) {
            return Some(request.dest);
        }
        let from = world.get(ids.body, mover)?.0.position;
        let class = self.mover_class(tick, ids, mover);
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let facts = self.kind_facts(world, ids, mover);
        let horde = world.contains(ids.horde, mover);
        let click = request.group_goal.unwrap_or(request.dest);
        let wall_layer = crate::wall_layer::layer_of(world, ids.wall_layer);
        let layer_of = |point: Vec2| {
            wall_layer.map_or(crate::wall_layer::Layer::GROUND, |layer| {
                layer.layer_at(geometry, extent, point)
            })
        };
        let layer = layer_of(click);
        let on_ground = !layer.is_wall_top();
        let heading = Facing::from_vector(request.dest.minus(from)).map_or(0.0, Facing::radians);
        let asker = self.destination_asker(tick, ids, mover);
        order.prepare(self, asker);
        let body = self
            .claim_at(
                tick,
                ids,
                mover,
                request.dest,
                Facing::from_radians(heading),
            )
            .map(|(_, body, offset)| (body, offset));
        let footprint = self.adjust_footprint(tick, ids, mover, body.as_ref(), on_ground, horde);
        let rectangle = facts
            .large_rectangle_pathfind
            .then(|| record.and_then(|record| self.rectangle_cells_of(tick, ids, mover, record)))
            .flatten();
        #[allow(clippy::cast_precision_loss)]
        let reach_cells = (self.max_cells_adjust_destination as f32).sqrt().ceil() + 3.0;
        let probe = Probe {
            rules: self,
            ids,
            world,
            class,
            confined: self.restriction(world, ids).confine(class),
            view: self.dynamic_view(tick, ids),
            extent,
            start: self.passable_cell_for(tick, ids, from, class),
            aircraft: record.is_some_and(|record| record.has_kind("AIRCRAFT")),
            dozer: facts.dozer,
            footprint,
            rectangle,
            heading,
            on_ground,
            horde,
            goal: self.passable_cell_for(tick, ids, request.dest, class),
            layer_tag: wall_layer.map_or(crate::wall_layer::Layer::GROUND.0, |layer| {
                layer
                    .tag_at(extent, self.passability.cell_at(click).unwrap_or_default())
                    .0
            }),
            claims: self.unit_cells(
                tick,
                ids,
                mover,
                None,
                request.dest,
                reach_cells * geometry.cell_size(),
            ),
            body,
            asker,
        };
        let ground = |point: Vec2| self.ground_height(point.x, point.y);
        let reference = if request.group_goal.is_none()
            && record.is_some_and(|record| record.has_kind("SHIP"))
        {
            self.flooded_height(click)
        } else {
            ground(click)
        };
        let found = probe.search(request.dest, reference, order);
        let Some(mut dest) = found else {
            if request.group_goal.is_some() {
                return probe
                    .search(request.dest, ground(request.dest), order)
                    .map(|_| request.dest);
            }
            return None;
        };
        if let Some(goal) = request.group_goal {
            dest = self.tighten_path(&probe, dest, goal, order);
        }
        Some(dest)
    }

    fn flooded_height(&self, point: Vec2) -> f32 {
        let flooded = self.passability.water_depth(point) > 0.0;
        match flooded
            .then(|| self.passability.water_surface_at(point))
            .flatten()
        {
            Some(surface) => surface,
            None => self.ground_height(point.x, point.y),
        }
    }

    pub(super) fn claim_facing(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        mover: Entity,
        heading: Facing,
    ) -> Facing {
        let rectangle = self
            .kind_facts(tick.state(), ids, mover)
            .large_rectangle_pathfind;
        if rectangle {
            Facing::from_radians(quantise_to_twelfths(heading.radians()))
        } else {
            heading
        }
    }

    pub(super) fn claim_cells(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        mover: Entity,
        at: Vec2,
        heading: f32,
    ) -> Vec<(i32, i32)> {
        let world = tick.state();
        let geometry = self.passability.geometry;
        let facts = self.kind_facts(world, ids, mover);
        let horde = world.contains(ids.horde, mover);
        let record = self.record_of(tick, ids, mover);
        let wall_layer = crate::wall_layer::layer_of(world, ids.wall_layer);
        let on_ground = wall_layer.is_none_or(|layer| {
            !layer
                .layer_at(geometry, self.passability.grid.extent(), at)
                .is_wall_top()
        });
        let body = self
            .claim_at(tick, ids, mover, at, Facing::from_radians(heading))
            .map(|(_, body, offset)| (body, offset));
        let footprint = self.adjust_footprint(tick, ids, mover, body.as_ref(), on_ground, horde);
        let centre = goal_cells::anchor(geometry, at, footprint);
        match facts
            .large_rectangle_pathfind
            .then(|| record.and_then(|record| self.rectangle_cells_of(tick, ids, mover, record)))
            .flatten()
        {
            Some(size) => {
                super::cell_raster::rectangle_cells(centre, size, quantise_to_twelfths(heading))
            }
            None => super::melee_engage::footprint_cells(self, at, footprint),
        }
    }

    pub(super) fn claim_horde_cells(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde: Entity,
        at: Vec2,
        facing: Facing,
    ) -> Vec<(i32, i32)> {
        let world = tick.state();
        let Some(def) = world.get(ids.horde, horde) else {
            return Vec::new();
        };
        let basis = mint_sage_formation::FormationBasis::of(facing);
        let mut cells = Vec::new();
        for &member in &def.members {
            let slot = world.get(ids.member, member).and_then(|entry| entry.slot);
            let (Some(offset), true) = (
                slot.and_then(|slot| def.offset(slot)),
                !world.get(ids.health, member).is_some_and(|h| h.0.is_dead()),
            ) else {
                continue;
            };
            cells.extend(self.claim_cells(
                tick,
                ids,
                member,
                basis.place(at, offset),
                facing.radians(),
            ));
        }
        cells
    }

    fn adjust_footprint(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        mover: Entity,
        body: Option<&(Body, Vec2)>,
        on_ground: bool,
        horde: bool,
    ) -> Footprint {
        let facts = self.kind_facts(tick.state(), ids, mover);
        if horde {
            if !on_ground {
                return Footprint::ONE_CENTRED;
            }
            if let Some((body, _)) = body {
                let pathfind_diameter = self
                    .record_of(tick, ids, mover)
                    .and_then(|record| record.pathfind_diameter);
                return Footprint::of(
                    body.enclosing_radius(),
                    true,
                    self.passability.geometry.cell_size(),
                    pathfind_diameter,
                );
            }
        }
        if facts.infantry && facts.hero && !on_ground {
            return Footprint::RADIUS_ONE;
        }
        self.footprint_of(tick, ids, mover)
    }

    fn rectangle_cells_of(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        mover: Entity,
        record: &mint_sage_modules::ObjectRecord,
    ) -> Option<(i32, i32)> {
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
        let world = tick.state();
        let slots = world.get(ids.horde, mover).and_then(|horde| {
            let slots = self
                .formations
                .get(usize::from(horde.formation))?
                .settings()
                .slots;
            let unexited = world
                .get(ids.production_stream, mover)
                .map_or(0, |stream| usize::try_from(stream.remaining).unwrap_or(0));
            (slots > 0).then_some((horde.members.len().saturating_add(unexited), slots))
        });
        #[allow(clippy::cast_precision_loss)]
        let fill = slots.map(|(held, slots)| held as f32 / slots as f32);
        #[allow(clippy::cast_possible_truncation)]
        let side = |radius: f32| {
            let cells = ((2.0 * radius + LARGE_RECTANGLE_SIDE_BIAS) * CELLS_PER_UNIT)
                .clamp(0.0, 1.0e4) as i32;
            match fill {
                Some(f) => ((cells as f32 - 2.0) * f + 2.0).clamp(0.0, 1.0e4) as i32,
                None => cells,
            }
        };
        Some((side(major), side(minor)))
    }

    fn tighten_path(&self, probe: &Probe<'_>, dest: Vec2, goal: Vec2, order: &OrderClaims) -> Vec2 {
        let (Some(from), Some(to)) = (
            self.passability.cell_at(dest),
            self.passability.cell_at(goal),
        ) else {
            return dest;
        };
        let mut best = dest;
        let mut asked = probe.goal;
        for (x, y) in cells_along_line(
            (i32::from(from.x), i32::from(from.y)),
            (i32::from(to.x), i32::from(to.y)),
        )
        .into_iter()
        .skip(1)
        {
            let (Ok(cx), Ok(cy)) = (u16::try_from(x), u16::try_from(y)) else {
                break;
            };
            if !probe.extent.contains(Cell::new(cx, cy)) {
                break;
            }
            if !probe.on_layer(&[(x, y)]) {
                continue;
            }
            if let Some(at) = probe.accepts((x, y), None, asked, order, probe.confined) {
                best = at;
                asked = self.passability.cell_at(at).or(asked);
            }
        }
        best
    }
}

fn cells_along_line(start: (i32, i32), end: (i32, i32)) -> Vec<(i32, i32)> {
    let dx = (end.0 - start.0).abs();
    let dy = (end.1 - start.1).abs();
    let step = |a: i32, b: i32| if b >= a { 1 } else { -1 };
    let (sx, sy) = (step(start.0, end.0), step(start.1, end.1));
    let steps = dx.max(dy);
    let mut cells = Vec::new();
    let (mut x, mut y) = start;
    let mut error = steps / 2;
    for _ in 0..=steps {
        cells.push((x, y));
        if dx >= dy {
            error += dy;
            if error >= dx {
                error -= dx;
                y += sy;
            }
            x += sx;
        } else {
            error += dx;
            if error >= dy {
                error -= dy;
                x += sx;
            }
            y += sy;
        }
    }
    cells
}

impl Probe<'_> {
    fn accepts(
        &self,
        cell: (i32, i32),
        height: Option<f32>,
        reach_from: Option<Cell>,
        order: &OrderClaims,
        class: mint_movement::MoverClass,
    ) -> Option<Vec2> {
        let rules = self.rules;
        let geometry = rules.passability.geometry;
        let standing = Cell::new(u16::try_from(cell.0).ok()?, u16::try_from(cell.1).ok()?);
        if !self.extent.contains(standing)
            || (self.on_ground
                && mint_movement::Passability::flags(&rules.passability.grid, standing)
                    .intersects(pass::CLIFF))
        {
            return None;
        }
        let narrow = self.horde
            && self.on_ground
            && mint_movement::Passability::flags(&rules.passability.grid, standing)
                .intersects(pass::NARROW_PASSAGE);
        let footprint = if narrow {
            Footprint::ONE_CENTRED
        } else {
            self.footprint
        };
        let cells = match self.rectangle {
            Some(size) => super::cell_raster::rectangle_cells(cell, size, self.heading),
            None => {
                let rect = goal_cells::rect(self.extent, cell, footprint)?;
                let (x0, y0, x1, y1) = rect.span();
                (y0..y1)
                    .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                    .collect()
            }
        };
        let passable = |&(x, y): &(i32, i32)| {
            let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                return false;
            };
            let at = Cell::new(x, y);
            self.extent.contains(at) && self.view.is_passable(at, class)
        };
        if !cells.iter().all(passable)
            || cells.iter().any(|c| order.taken.contains(c))
            || !self.claims.destination_clear(cells.iter().copied())
        {
            return None;
        }
        let answer = goal_cells::answer_point(geometry, cell, footprint);
        if let Some((body, offset)) = &self.body {
            let crowded = order
                .bodies
                .as_ref()
                .filter(|(held, _)| *held == self.asker)
                .is_some_and(|(_, claims)| claims.crowded(&body.at(answer.plus(*offset))));
            if crowded {
                return None;
            }
        }
        if self.aircraft {
            return Some(answer);
        }
        if self.dozer && !self.on_layer(&cells) {
            return None;
        }
        let answer_height = if self.on_ground {
            rules.flooded_height(answer)
        } else {
            rules.ground_height(answer.x, answer.y)
        };
        if height
            .is_some_and(|wanted| wanted > 0.0 && (answer_height - wanted).abs() > MAX_HEIGHT_STEP)
        {
            return None;
        }
        self.reaches(standing, reach_from).then_some(answer)
    }

    fn on_layer(&self, cells: &[(i32, i32)]) -> bool {
        let Some(layer) = crate::wall_layer::layer_of(self.world, self.ids.wall_layer) else {
            return true;
        };
        cells.iter().all(|&(x, y)| {
            let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                return false;
            };
            layer.tag_at(self.extent, Cell::new(x, y)).0 == self.layer_tag
        })
    }

    fn reaches(&self, answer: Cell, asked: Option<Cell>) -> bool {
        let Some(start) = self.start else {
            return true;
        };
        let reach = |from: Cell| self.rules.reachable(self.class, from, answer);
        reach(start)
            || asked.is_some_and(|asked| {
                !self.rules.reachable(self.class, start, asked) && reach(asked)
            })
    }

    fn search(&self, dest: Vec2, height: f32, order: &OrderClaims) -> Option<Vec2> {
        let rules = self.rules;
        let geometry = rules.passability.geometry;
        let (gx, gy) = goal_cells::anchor(geometry, dest, self.footprint);
        let mut asked = self.goal;
        let mut accepts = |dx: i32, dy: i32| {
            let cell = (gx.checked_add(dx)?, gy.checked_add(dy)?);
            let at = self.accepts(cell, Some(height), asked, order, self.class)?;
            asked = rules.passability.cell_at(at).or(asked);
            Some(at)
        };
        if let Some(at) = accepts(0, 0) {
            return Some(at);
        }
        let mut best: Option<(i32, Vec2)> = None;
        let mut budget = i64::from(rules.max_cells_adjust_destination);
        for round in square_spiral() {
            if budget <= 0 {
                break;
            }
            budget = budget.saturating_sub(i64::try_from(round.len()).unwrap_or(i64::MAX));
            for (dx, dy) in round {
                let distance = dx * dx + dy * dy;
                if best.is_some_and(|(least, _)| distance >= least) {
                    continue;
                }
                if let Some(at) = accepts(dx, dy) {
                    best = Some((distance, at));
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.map(|(_, at)| at)
    }
}

pub(super) fn square_spiral() -> impl Iterator<Item = Vec<(i32, i32)>> {
    let mut cursor = (0i32, 0i32);
    (1..=SPIRAL_ROUNDS).map(move |round| {
        let side = 2 * round - 1;
        let mut cells = Vec::new();
        for (steps, dx, dy) in [
            (side, 1, 0),
            (side, 0, 1),
            (side + 1, -1, 0),
            (side + 1, 0, -1),
        ] {
            for _ in 0..steps {
                cursor = (cursor.0 + dx, cursor.1 + dy);
                cells.push(cursor);
            }
        }
        cells
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spiral_is_zh_rounds_not_rings() {
        let rounds: Vec<Vec<(i32, i32)>> = square_spiral().take(2).collect();
        assert_eq!(
            rounds[0],
            [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)]
        );
        assert_eq!(rounds[1].len(), 14);
        assert_eq!(rounds[1][..3], [(0, -1), (1, -1), (2, -1)]);
    }
}
