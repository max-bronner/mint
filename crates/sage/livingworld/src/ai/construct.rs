use mint_math::f32::ln;

use crate::ai::snapshot::{RegionRec, FAR};
use crate::ai::Ctx;
use crate::model::{bonus, BuildingId, RegionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildAction {
    Build(BuildingId),
    Demolish { plot: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildOrder {
    pub region: RegionId,
    pub action: BuildAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Farm,
    Barracks,
    Armory,
    Fortress,
}

struct Wants {
    income: i32,
    barracks: i32,
    armory: i32,
    no_fortress: bool,
}

fn enemy_hero_distance(ctx: &Ctx<'_>, at: RegionId) -> u16 {
    ctx.snap
        .nearest(at, false, |a| a.hero && !ctx.state.allied(ctx.me, a.player))
        .map_or(FAR, |(_, d)| d)
}

fn own_hero_distance(ctx: &Ctx<'_>, at: RegionId) -> u16 {
    ctx.snap
        .nearest(at, false, |a| a.hero && a.player == ctx.me)
        .map_or(FAR, |(_, d)| d)
}

fn count(n: usize) -> f32 {
    f32::from(u16::try_from(n).unwrap_or(u16::MAX))
}

fn noisy(ctx: &mut Ctx<'_>, s: i32) -> f32 {
    let s = s as f32;
    s + ctx.rolls.real(0.0, s)
}

fn farm_score(ctx: &mut Ctx<'_>, rec: &RegionRec, want: i32) -> f32 {
    let s = ctx.template.farm;
    if want <= 0 || s <= 0 {
        return 0.0;
    }
    noisy(ctx, s) * 1.5 * f32::from(enemy_hero_distance(ctx, rec.id))
}

fn armory_score(ctx: &mut Ctx<'_>, rec: &RegionRec, want: i32) -> f32 {
    let s = ctx.template.armory;
    if want <= 0 || s <= 0 {
        return 0.0;
    }
    let half = f32::from(own_hero_distance(ctx, rec.id)) * 0.5;
    let mut v = noisy(ctx, s) * 1.5 * (3.0 - (half * half).min(3.0));
    v *= if rec.spawn_only.is_empty() {
        0.5
    } else {
        count(rec.spawn_only.len()) + 1.0
    };
    if rec.upgrade.is_empty() {
        v
    } else {
        0.0
    }
}

fn barracks_score(ctx: &mut Ctx<'_>, rec: &RegionRec, want: i32) -> f32 {
    let s = ctx.template.barracks;
    if want <= 0 || s <= 0 {
        return 0.0;
    }
    let base = noisy(ctx, s);
    if want > 3 {
        return 5.0 * want as f32 * base;
    }
    let enemy = f32::from(enemy_hero_distance(ctx, rec.id)) - 2.5;
    let own = f32::from(own_hero_distance(ctx, rec.id));
    base * (3.0 - 0.5 * enemy * enemy) * (3.0 - 0.5 * own)
}

fn fortress_score(ctx: &mut Ctx<'_>, rec: &RegionRec, wants: &Wants) -> f32 {
    let mut v = noisy(ctx, ctx.template.castle);
    for built in &rec.strengthen {
        let level = ctx
            .state
            .rules
            .ai
            .building(built.building)
            .map_or(0.0, |b| b.strengthen_level_one);
        v *= (1.0 - 0.1 * level).max(0.1);
    }
    let n = enemy_hero_distance(ctx, rec.id);
    if n < 6 {
        let nf = f32::from(n);
        v *= nf * ln(6.0 - nf);
        let m = own_hero_distance(ctx, rec.id);
        if m > 0 {
            v *= 2.0 / (f32::from(m) + 1.0) + 0.5;
        }
    } else {
        v *= 0.1;
    }
    let plots = ctx.state.region(rec.id).map_or(0, |r| r.plots.len());
    if plots == 1 || plots == 3 {
        v *= 2.0;
    }
    if wants.no_fortress {
        v *= 2.0;
    }
    if wants.income > 0 && ctx.template.farm > 0 {
        v += 0.5 * f32::from(n.max(1)) * ctx.template.farm as f32 * 2.0;
    }
    v
}

fn role(ctx: &Ctx<'_>, class: Class) -> Option<BuildingId> {
    let r = ctx.catalogue.roles;
    match class {
        Class::Farm => r.farm,
        Class::Barracks => r.barracks,
        Class::Armory => r.armory,
        Class::Fortress => r.fortress,
    }
}

fn can_build(ctx: &Ctx<'_>, rec: &RegionRec, class: Class) -> bool {
    role(ctx, class).is_some_and(|b| !ctx.state.build_restricted(rec.id, b))
}

fn has_class(rec: &RegionRec, class: Class) -> bool {
    match class {
        Class::Farm => !rec.world_cp.is_empty(),
        Class::Barracks => !rec.spawn_only.is_empty(),
        Class::Armory => !rec.upgrade.is_empty(),
        Class::Fortress => !rec.strengthen.is_empty(),
    }
}

fn cost(ctx: &Ctx<'_>, class: Class) -> i32 {
    let discount = ctx
        .state
        .player(ctx.me)
        .map_or(0, |p| p.bonus_a.get(bonus::BUILDING_DISCOUNT));
    role(ctx, class)
        .and_then(|b| ctx.state.rules.building(b))
        .map_or(0, |b| b.cost.saturating_sub(discount))
        .max(0)
}

fn pick_empty(f: f32, b: f32, a: f32, c: f32) -> Class {
    if f >= a && f >= c && f >= b {
        Class::Farm
    } else if b >= a && b >= c {
        Class::Barracks
    } else if a >= c {
        Class::Armory
    } else {
        Class::Fortress
    }
}

fn pick_replace(f: f32, b: f32, a: f32, c: f32) -> Class {
    if b >= f && b >= a && b >= c {
        Class::Barracks
    } else if f >= a && f >= c {
        Class::Farm
    } else if a >= c {
        Class::Armory
    } else {
        Class::Fortress
    }
}

fn total(ctx: &Ctx<'_>, pick: impl Fn(&RegionRec) -> usize) -> i32 {
    let regions = ctx
        .snap
        .summary(ctx.me)
        .map(|s| s.regions.as_slice())
        .unwrap_or(&[]);
    let n: usize = regions
        .iter()
        .filter_map(|r| ctx.snap.region(*r))
        .map(pick)
        .sum();
    i32::try_from(n).unwrap_or(i32::MAX)
}

pub(crate) fn plan(ctx: &mut Ctx<'_>, mut budget: i32) -> Vec<BuildOrder> {
    let summary = ctx.snap.summary(ctx.me).cloned().unwrap_or_default();
    let t = role(ctx, Class::Farm)
        .and_then(|b| ctx.state.rules.ai.building(b))
        .and_then(|b| b.treasury)
        .filter(|t| *t != 0)
        .unwrap_or(500);
    let owned = i32::try_from(summary.regions.len()).unwrap_or(i32::MAX);
    let barracks = total(ctx, |r| r.spawn_only.len());
    let armories = total(ctx, |r| r.upgrade.len());
    let fortresses = total(ctx, |r| r.strengthen.len());
    let mut wants = Wants {
        income: owned
            .saturating_add(1)
            .saturating_mul(t)
            .saturating_sub(summary.income)
            .checked_div(t)
            .unwrap_or(0)
            .max(0),
        barracks: summary
            .income
            .checked_div(t)
            .unwrap_or(0)
            .saturating_sub(barracks)
            .max(0),
        armory: barracks
            .saturating_sub(armories)
            .saturating_mul(2)
            .checked_div(4)
            .unwrap_or(0),
        no_fortress: fortresses <= 0,
    };
    let mut orders = Vec::new();
    for region in &summary.regions {
        let Some(rec) = ctx.snap.region(*region) else {
            continue;
        };
        if rec.under_construction {
            continue;
        }
        let f = if can_build(ctx, rec, Class::Farm) {
            farm_score(ctx, rec, wants.income)
        } else {
            0.0
        };
        let b = if can_build(ctx, rec, Class::Barracks) {
            barracks_score(ctx, rec, wants.barracks)
        } else {
            0.0
        };
        let a = if can_build(ctx, rec, Class::Armory) {
            armory_score(ctx, rec, wants.armory)
        } else {
            0.0
        };
        let c = if can_build(ctx, rec, Class::Fortress) {
            fortress_score(ctx, rec, &wants)
        } else {
            0.0
        };
        let mut class = if rec.empty_plots > 0 {
            pick_empty(f, b, a, c)
        } else {
            let mut keeps: [(Class, f32); 4] = [
                (Class::Barracks, -1.0),
                (Class::Farm, -1.0),
                (Class::Armory, -1.0),
                (Class::Fortress, -1.0),
            ];
            let farm_keep = if has_class(rec, Class::Farm) {
                farm_score(ctx, rec, wants.income.saturating_add(1))
            } else {
                -1.0
            };
            let barracks_keep = if has_class(rec, Class::Barracks) {
                barracks_score(ctx, rec, wants.barracks.saturating_add(1))
            } else {
                -1.0
            };
            let armory_keep = if has_class(rec, Class::Armory) {
                armory_score(ctx, rec, wants.armory)
            } else {
                -1.0
            };
            let fortress_keep = if has_class(rec, Class::Fortress) {
                0.0
            } else {
                -1.0
            };
            for (slot, value) in
                keeps
                    .iter_mut()
                    .zip([barracks_keep, farm_keep, armory_keep, fortress_keep])
            {
                slot.1 = value;
            }
            let keep = keeps.iter().fold(-1.0f32, |m, k| m.max(k.1));
            let best = a.max(c).max(b).max(f);
            if !(best > 1.5 * keep && best > 20.0 && keep >= 0.0 && ctx.rolls.int(0, 1) == 1) {
                continue;
            }
            let Some(victim) = keeps
                .iter()
                .copied()
                .reduce(|v, k| if k.1 > v.1 { k } else { v })
            else {
                continue;
            };
            let plot = match victim.0 {
                Class::Barracks => rec.spawn_only.first(),
                Class::Farm => rec.world_cp.first(),
                Class::Armory => rec.upgrade.first(),
                Class::Fortress => rec.strengthen.first(),
            };
            if let Some(plot) = plot {
                orders.push(BuildOrder {
                    region: rec.id,
                    action: BuildAction::Demolish { plot: plot.plot },
                });
            }
            match victim.0 {
                Class::Barracks => wants.barracks = wants.barracks.saturating_add(1),
                Class::Farm => wants.income = wants.income.saturating_add(1),
                Class::Armory => wants.armory = wants.armory.saturating_add(1),
                Class::Fortress => {}
            }
            pick_replace(f, b, a, c)
        };
        let mut price = cost(ctx, class);
        if price > budget && price != 0 {
            class = Class::Farm;
            price = cost(ctx, class);
        }
        let Some(building) = role(ctx, class).filter(|_| can_build(ctx, rec, class)) else {
            continue;
        };
        orders.push(BuildOrder {
            region: rec.id,
            action: BuildAction::Build(building),
        });
        budget = budget.saturating_sub(price);
        match class {
            Class::Farm => wants.income = wants.income.saturating_sub(1),
            Class::Barracks => wants.barracks = wants.barracks.saturating_sub(1),
            Class::Armory => wants.armory = wants.armory.saturating_sub(1),
            Class::Fortress => {
                wants.income = wants.income.saturating_sub(1);
                wants.no_fortress = false;
            }
        }
    }
    orders
}
