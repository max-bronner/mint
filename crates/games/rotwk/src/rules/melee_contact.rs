use mint_movement::Vec2;
use mint_sim::{Entity, Tick, World};

use super::{Ids, RotwkRules};
use crate::goal_cells::{self, Footprint};

const CLIMBING_LOOSE: [&str; 2] = ["CLIMBING", "RAPPELLING"];

const CLIMBING_MELEE_REACH_SQUARED: f32 = 5.0;

const TOP_ATTACK_HEIGHT: f32 = 20.0;

const WALL_TOP_REACH: f32 = 20.0;

fn layers_allow_strike(
    attacker: crate::wall_layer::Layer,
    target: crate::wall_layer::Layer,
    live_portal: bool,
) -> bool {
    use crate::wall_layer::Layer;
    if attacker == Layer::GROUND && target.is_wall_top() {
        return false;
    }
    if attacker.is_wall_top() && target == Layer::GROUND {
        return live_portal;
    }
    true
}

fn top_attack_point(tower: Vec2, facing: Vec2, (px, py): (f32, f32)) -> Vec2 {
    let widest = facing.x.abs().max(facing.y.abs());
    let stretch = if widest > 0.0 { 1.0 / widest } else { 1.0 };
    let (px, py) = (px * stretch, py * stretch);
    Vec2::new(
        tower.x + px * facing.x - py * facing.y,
        tower.y + px * facing.y + py * facing.x,
    )
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Striker {
    pub(super) entity: Entity,
    pub(super) at: Vec2,
    pub(super) footprint: Footprint,
}

pub(crate) fn reach_cells(rv: f32) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let cells = (rv * 0.1 + 0.1).floor().clamp(0.0, 1000.0) as i32;
    cells
}

fn reach_on(
    template: &mint_sage_combat::WeaponTemplate,
    bonus: f32,
    victim: Option<&mint_sage_combat::Attributes>,
) -> f32 {
    let trimmed = template.ranged_reach(bonus);
    victim.map_or(trimmed, |attributes| {
        attributes.range(attributes.range(trimmed))
    })
}

fn edge_squared(from: Vec2, radius: f32, to: Vec2, to_radius: f32) -> f32 {
    mint_sage_combat::surface_distance_squared_2d(
        mint_sage_combat::Point3::new(from.x, from.y, 0.0),
        radius,
        mint_sage_combat::Point3::new(to.x, to.y, 0.0),
        to_radius,
    )
}

fn box_distance_squared(point: Vec2, wall: &mint_sage_passability::WallGeometry) -> f32 {
    let (sin, cos) = mint_math::f32::sin_cos(wall.angle);
    let dx = point.x - wall.centre.x;
    let dy = point.y - wall.centre.y;
    let local_x = dx * cos + dy * sin;
    let local_y = -dx * sin + dy * cos;
    let out_x = (local_x.abs() - wall.major_radius).max(0.0);
    let out_y = (local_y.abs() - wall.minor_radius).max(0.0);
    out_x * out_x + out_y * out_y
}

impl RotwkRules {
    fn current_melee_weapon<'a>(
        &'a self,
        world: &World,
        ids: Ids,
        shooter: Entity,
    ) -> Option<&'a mint_sage_combat::WeaponTemplate> {
        world
            .get(ids.armed, shooter)
            .and_then(|armed| self.combat.weapons.get(armed.weapon))
            .filter(|template| template.melee)
    }

    fn melee_reach_on(
        &self,
        world: &World,
        ids: Ids,
        victim: Entity,
        template: &mint_sage_combat::WeaponTemplate,
    ) -> f32 {
        let bonus = self.weapon_bonus(world, ids, victim, template).range();
        reach_on(
            template,
            bonus,
            world.get(ids.bonuses, victim).map(|b| &b.0),
        )
    }

    pub(super) fn melee_contact(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        striker: Striker,
        victim: Entity,
    ) -> bool {
        let world = tick.state();
        let Some(template) = self.current_melee_weapon(world, ids, striker.entity) else {
            return false;
        };
        let (Some(victim_at), Some(victim_shape), Some(shooter_shape)) = (
            world.get(ids.body, victim).map(|body| body.0.position),
            world.get(ids.shape, victim),
            world.get(ids.shape, striker.entity),
        ) else {
            return false;
        };
        let geometry = self.passability.geometry;
        let k = reach_cells(self.melee_reach_on(world, ids, victim, template));
        let anchor = goal_cells::anchor(geometry, striker.at, striker.footprint);
        let current = world
            .get(ids.body, striker.entity)
            .map_or(striker.at, |body| body.0.position);
        if victim_shape
            .kind_of
            .contains(mint_sage_asset::KindOf::STRUCTURE)
        {
            return self.structure_in_contact(tick, ids, (striker, anchor, k), victim);
        }
        let kind_of = victim_shape.kind_of;
        if kind_of.contains(mint_sage_asset::KindOf::CAN_CLIMB_WALLS)
            && self.is_climbing(tick, ids, victim)
        {
            return edge_squared(
                current,
                shooter_shape.radius,
                victim_at,
                victim_shape.radius,
            ) < CLIMBING_MELEE_REACH_SQUARED;
        }
        let theirs = self.footprint_of(tick, ids, victim);
        if goal_cells::within_gap(
            anchor,
            striker.footprint,
            goal_cells::anchor(geometry, victim_at, theirs),
            theirs,
            k,
        ) {
            return true;
        }
        kind_of.contains(mint_sage_asset::KindOf::SIEGE_TOWER)
            && self.top_attack_reached(world, ids, striker, shooter_shape.radius, victim)
    }

    fn is_climbing(&self, tick: &Tick<'_>, ids: Ids, victim: Entity) -> bool {
        let world = tick.state();
        let horde = if world.contains(ids.horde, victim) {
            Some(victim)
        } else {
            world.get(ids.member, victim).map(|entry| entry.horde)
        };
        match horde {
            Some(horde) => world
                .get(ids.horde, horde)
                .is_some_and(|horde| horde.climb_wall.is_some()),
            None => CLIMBING_LOOSE
                .iter()
                .any(|name| self.holds_model_condition(tick, victim, name)),
        }
    }

    fn top_attack_reached(
        &self,
        world: &World,
        ids: Ids,
        striker: Striker,
        radius: f32,
        victim: Entity,
    ) -> bool {
        let Some(portal) = self
            .modules
            .as_ref()
            .and_then(|wiring| wiring.dynamic_portal(world, victim))
        else {
            return false;
        };
        let (Some((px, py, pz)), Some(reach)) =
            (portal.top_attack_pos(), portal.top_attack_radius())
        else {
            return false;
        };
        if !Self::siege_deployed(world, ids, victim) {
            return false;
        }
        let Some(body) = world.get(ids.body, victim) else {
            return false;
        };
        let top = top_attack_point(body.0.position, body.0.facing.to_vector(), (px, py));
        let top_z = Self::feet(world, ids, victim) + pz;
        let limit = radius + reach;
        (top_z - Self::feet(world, ids, striker.entity)).abs() < TOP_ATTACK_HEIGHT
            && (top.x - striker.at.x).abs() < limit
            && (top.y - striker.at.y).abs() < limit
    }

    fn structure_in_contact(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        (striker, anchor, k): (Striker, (i32, i32), i32),
        victim: Entity,
    ) -> bool {
        let world = tick.state();
        let cells = crate::obstacles::layer_of(world, ids.obstacles).and_then(|layer| {
            layer.cells_of(
                self.passability.geometry,
                self.passability.grid.extent(),
                victim,
            )
        });
        if cells.is_some_and(|cells| {
            goal_cells::rings_hold(anchor, striker.footprint, k.saturating_add(1), &cells)
        }) {
            return true;
        }
        if !self.has_kind_bit(world, ids, victim, "WALK_ON_TOP_OF_WALL") {
            return false;
        }
        self.has_kind_bit(world, ids, victim, "WALL_UPGRADE")
            && crate::obstacles::scaleable_walls_of(world, ids.scaleable_walls)
                .and_then(|walls| walls.iter().find(|(entity, _)| *entity == victim))
                .is_some_and(|(_, wall)| {
                    box_distance_squared(striker.at, wall) < WALL_TOP_REACH * WALL_TOP_REACH
                })
    }

    pub(super) fn melee_in_range(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        striker: Striker,
        victim: Entity,
        running_down: bool,
    ) -> bool {
        let world = tick.state();
        let (Some(victim_shape), Some(victim_at), Some(shooter_shape)) = (
            world.get(ids.shape, victim),
            world.get(ids.body, victim).map(|body| body.0.position),
            world.get(ids.shape, striker.entity),
        ) else {
            return false;
        };
        let contesting = |entity| {
            self.modules
                .as_ref()
                .is_some_and(|wiring| wiring.carries_status(world, entity, "CONTESTING_BUILDING"))
        };
        if contesting(striker.entity) && contesting(victim) {
            return true;
        }
        let structure = victim_shape
            .kind_of
            .contains(mint_sage_asset::KindOf::STRUCTURE);
        if !structure
            && (!self.has_ai(world, ids, victim)
                || self.current_locomotor(tick, ids, victim).is_none())
        {
            return self.melee_as_ranged(tick, ids, striker, victim);
        }
        let current = world
            .get(ids.body, striker.entity)
            .map_or(striker.at, |body| body.0.position);
        let limit = self.melee_placement.chase_from_behind;
        if running_down
            && edge_squared(
                current,
                shooter_shape.radius,
                victim_at,
                victim_shape.radius,
            ) < limit * limit
        {
            return true;
        }
        if !self.melee_contact(tick, ids, striker, victim) {
            return false;
        }
        if self.has_kind_bit(world, ids, victim, "WALL_UPGRADE") {
            return true;
        }
        let layer = |entity| {
            world
                .get(ids.layer, entity)
                .copied()
                .unwrap_or(crate::wall_layer::Layer::GROUND)
        };
        let live_portal = crate::portals::layer_in(world, ids.portals)
            .is_some_and(|links| links.holds(victim))
            && self
                .modules
                .as_ref()
                .is_some_and(|wiring| wiring.dynamic_portal(world, victim).is_some());
        layers_allow_strike(layer(striker.entity), layer(victim), live_portal)
    }

    fn melee_as_ranged(&self, tick: &Tick<'_>, ids: Ids, striker: Striker, victim: Entity) -> bool {
        let world = tick.state();
        let Some(template) = self.current_melee_weapon(world, ids, striker.entity) else {
            return false;
        };
        let (Some(victim_at), Some(victim_shape), Some(shooter_shape)) = (
            world.get(ids.body, victim).map(|body| body.0.position),
            world.get(ids.shape, victim),
            world.get(ids.shape, striker.entity),
        ) else {
            return false;
        };
        let trimmed = template.ranged_reach(
            self.weapon_bonus(world, ids, striker.entity, template)
                .range(),
        );
        let max = world
            .get(ids.bonuses, striker.entity)
            .map_or(trimmed, |bonuses| bonuses.0.range(trimmed));
        crate::combat::in_range(
            striker.at,
            shooter_shape.radius,
            victim_at,
            victim_shape.radius,
            self.min_range_of(world, striker.entity, template),
            max,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_combat::{Attributes, WeaponTemplate};

    fn blade(attack_range: f32) -> WeaponTemplate {
        WeaponTemplate {
            attack_range,
            melee: true,
            ..WeaponTemplate::default()
        }
    }

    #[test]
    fn the_top_attack_point_is_stretched_for_a_turned_tower() {
        let at = top_attack_point(Vec2::ZERO, Vec2::new(1.0, 0.0), (34.0, 0.0));
        assert!((at.x - 34.0).abs() < 1e-3 && at.y.abs() < 1e-3, "{at:?}");
        let half = core::f32::consts::FRAC_1_SQRT_2;
        let turned = top_attack_point(Vec2::ZERO, Vec2::new(half, half), (34.0, 0.0));
        assert!(
            (turned.x - 34.0).abs() < 1e-2 && (turned.y - 34.0).abs() < 1e-2,
            "{turned:?}"
        );
    }

    #[test]
    fn a_strike_crosses_between_a_wall_top_and_the_ground_only_at_a_live_portal() {
        use crate::wall_layer::Layer;
        let top = Layer(17);
        assert!(!layers_allow_strike(Layer::GROUND, top, true));
        assert!(!layers_allow_strike(top, Layer::GROUND, false));
        assert!(layers_allow_strike(top, Layer::GROUND, true));
        assert!(layers_allow_strike(Layer::GROUND, Layer::GROUND, false));
        assert!(layers_allow_strike(top, Layer(18), false));
    }

    fn k(attack_range: f32, victim_factor: f32) -> i32 {
        let victim = Attributes {
            range: victim_factor - 1.0,
            ..Attributes::default()
        };
        reach_cells(reach_on(&blade(attack_range), 1.0, Some(&victim)))
    }

    #[test]
    fn melee_reach_in_cells_follows_the_victims_trimmed_range() {
        assert_eq!(k(11.5, 1.0), 1);
        assert_eq!(k(30.0, 1.0), 2);
        assert_eq!(k(20.0, 1.0), 1);
        assert_eq!(k(11.5, 0.9), 0);
        assert_eq!(k(30.0, 0.8), 1);
    }

    fn infantry() -> Footprint {
        Footprint::of(8.0, false, 10.0, None)
    }

    #[test]
    fn a_unit_victim_is_in_contact_when_the_empty_cells_between_are_at_most_k() {
        let geometry = mint_movement::CellGeometry::new(10.0, Vec2::ZERO).unwrap();
        let at = |x| goal_cells::anchor(geometry, Vec2::new(x, 50.0), infantry());
        let within = |x, gap| goal_cells::within_gap(at(100.0), infantry(), at(x), infantry(), gap);
        assert!(within(130.0, 1));
        assert!(!within(140.0, 1));
        assert!(within(140.0, 2));
        assert!(!within(150.0, 2));
    }
}
