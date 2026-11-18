use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

const MIN_RADIUS: f32 = 50.0;
const MIN_VALUE_RADIUS: f32 = 40.0;
const COARSE_CELLS: i32 = 10;
const FINE_CELLS: i32 = 11;
const SCAN_ORDERS: u32 = 4;
const WEIGHT_DIVISOR_KINDS: [&str; 2] = ["COMMANDCENTER", "FS_SUPERWEAPON"];

struct Worth {
    at: Vec2,
    weight: f32,
}

fn value_at(worth: &[Worth], center: Vec2, radius: f32) -> i32 {
    let radius = radius.max(MIN_VALUE_RADIUS);
    let reach = radius * radius;
    let mut cash = 0.0_f32;
    for object in worth {
        let (dx, dy) = (center.x - object.at.x, center.y - object.at.y);
        if dx * dx + dy * dy < reach {
            let factor = 1.0 - (dx * dx + dy * dy).sqrt() / (2.0 * radius);
            cash += factor * object.weight;
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    let whole = cash as i32;
    whole
}

fn best_target(
    tick: &mut Tick<'_>,
    worth: &[Worth],
    bounds: (Vec2, Vec2),
    radius: f32,
) -> Option<Vec2> {
    let radius = radius.max(1.0);
    let (mut lo, mut hi) = bounds;
    lo.x += radius;
    hi.x -= radius;
    if hi.x < lo.x {
        let mid = (hi.x + lo.x) / 2.0;
        (lo.x, hi.x) = (mid, mid);
    }
    if hi.y < lo.y {
        let mid = (hi.y + lo.y) / 2.0;
        (lo.y, hi.y) = (mid, mid);
    }
    let (width, height) = (hi.x - lo.x, hi.y - lo.y);
    #[allow(clippy::cast_possible_truncation)]
    let count = |extent: f32| (((extent / radius).ceil() as i32) + 1).min(COARSE_CELLS);
    let (x_count, y_count) = (count(width), count(height));
    let (x_delta, y_delta) = match tick.rng().below(SCAN_ORDERS) {
        0 => (1, 1),
        1 => (-1, 1),
        2 => (1, -1),
        _ => (-1, -1),
    };
    let x_start = if x_delta > 0 { 0 } else { x_count };
    let y_start = if y_delta > 0 { 0 } else { y_count };
    #[allow(clippy::cast_precision_loss)]
    let as_f = |n: i32| n as f32;
    let mut cash = -1;
    let mut best = lo;
    let mut x_index = x_start;
    for _ in 0..x_count {
        let mut y_index = y_start;
        for _ in 0..y_count {
            let at = Vec2::new(
                lo.x + width * as_f(x_index) / as_f(x_count),
                lo.y + height * as_f(y_index) / as_f(y_count),
            );
            let here = value_at(worth, at, 2.0 * radius);
            if here > cash {
                cash = here;
                best = at;
            }
            y_index += y_delta;
        }
        x_index += x_delta;
    }
    let mut cash = -1;
    let mut chosen = best;
    let mut tied = 0_u32;
    for x in 0..FINE_CELLS {
        for _ in 0..FINE_CELLS {
            let step = as_f(x - 5) * (radius / 10.0);
            let at = Vec2::new(best.x + step, best.y + step);
            let here = value_at(worth, at, radius);
            if here > cash {
                cash = here;
                chosen = at;
                tied = 1;
            } else if here == cash {
                chosen = Vec2::new(chosen.x + at.x, chosen.y + at.y);
                tied += 1;
            }
        }
    }
    if tied > 1 {
        #[allow(clippy::cast_precision_loss)]
        let divisor = tied as f32;
        chosen = Vec2::new(chosen.x / divisor, chosen.y / divisor);
    }
    (cash > -1).then_some(chosen)
}

type Bounds = Option<(Vec2, Vec2)>;

fn widen(bounds: &mut Bounds, at: Vec2) {
    let (lo, hi) = bounds.get_or_insert((at, at));
    *lo = Vec2::new(lo.x.min(at.x), lo.y.min(at.y));
    *hi = Vec2::new(hi.x.max(at.x), hi.y.max(at.y));
}

impl RotwkRules {
    fn enemy_worth(&self, tick: &Tick<'_>, ids: Ids, enemy: PlayerId) -> (Vec<Worth>, Bounds) {
        let world = tick.state();
        let mut worth = Vec::new();
        let (mut structures, mut everything): (Bounds, Bounds) = (None, None);
        let Some(owners) = world.store(ids.owner) else {
            return (worth, None);
        };
        for (entity, owner) in owners.iter() {
            if owner.0 != enemy || !world.is_live(entity) || world.contains(ids.doomed, entity) {
                continue;
            }
            let Some(at) = self.position_of_entity(tick, ids, entity) else {
                continue;
            };
            widen(&mut everything, at);
            if self.has_kind_bit(world, ids, entity, "STRUCTURE") {
                widen(&mut structures, at);
            }
            if self.has_kind_bit(world, ids, entity, "AIRCRAFT")
                && self.airborne_category(world, ids, entity).is_some()
            {
                continue;
            }
            let cost = world
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
                .and_then(|entry| entry.record.build_cost)
                .unwrap_or(0.0);
            let divisions = WEIGHT_DIVISOR_KINDS
                .iter()
                .filter(|bit| self.has_kind_bit(world, ids, entity, bit))
                .count();
            let mut weight = cost;
            for _ in 0..divisions {
                weight /= 10.0;
            }
            worth.push(Worth { at, weight });
        }
        (worth, structures.or(everything))
    }

    fn ready_power_caster(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        power: &str,
    ) -> Option<Entity> {
        let world = tick.state();
        world
            .store(ids.owner)?
            .iter()
            .filter(|(entity, owner)| {
                owner.0 == player && world.is_live(*entity) && !world.contains(ids.doomed, *entity)
            })
            .map(|(entity, _)| entity)
            .find(|&entity| {
                world
                    .get(ids.kind, entity)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| entry.record.power(power).is_some())
                    && self.power_is_ready(world, ids, tick.frame(), entity, power)
            })
    }
}

struct FireAtMostCost {
    player_text: String,
    power: String,
}

impl ActionVerb for FireAtMostCost {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        current: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(player) = rules
            .player_mask(&state.rt, &self.player_text, current)
            .first()
        else {
            return Ok(());
        };
        let Some(enemy) = rules.script_local_player else {
            return Ok(());
        };
        let Some(caster) = rules.ready_power_caster(tick, ids, player, &self.power) else {
            return Ok(());
        };
        let radius = rules
            .power_names
            .id(&self.power)
            .and_then(|id| rules.power_cast_rules.get(usize::from(id.get())))
            .and_then(|rule| rule.radius_cursor)
            .unwrap_or(0.0)
            .max(MIN_RADIUS);
        let (worth, bounds) = rules.enemy_worth(tick, ids, enemy);
        let Some(bounds) = bounds else {
            return Ok(());
        };
        let Some(at) = best_target(tick, &worth, bounds, radius) else {
            return Ok(());
        };
        if at.x * at.x + at.y * at.y <= 0.0 {
            return Ok(());
        }
        rules.fire_power_scripted(tick, ids, caster, &self.power, None, Some((at.x, at.y)))?;
        state.rt.triggered_powers.push(player, &self.power, caster);
        diag!(
            "script: {} fired at the densest enemy cost ({:.0}, {:.0}) for player {}",
            self.power,
            at.x,
            at.y,
            player.get()
        );
        Ok(())
    }
}

pub(super) fn parse_fire_at_most_cost(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(FireAtMostCost {
        player_text: parameter_string(params, 0)?,
        power: parameter_string(params, 1)?,
    }))
}
