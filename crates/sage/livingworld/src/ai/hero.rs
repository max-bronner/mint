use crate::ai::snapshot::FAR;
use crate::ai::{Ctx, MoveKind, MoveOrder, Prediction, Scale};
use crate::battle::{BattleSpec, SpecSide};
use crate::model::{bonus, Army, ArmyId, PlayerId, RegionId};
use crate::victory::names_team;

const SEEN_RADIUS: u16 = 1;

const MAX_PATH_STEP_COST: u32 = 2;

fn is_ally(ctx: &Ctx<'_>, other: PlayerId) -> bool {
    ctx.state.allied(ctx.me, other)
}

fn has_move(moves: &[MoveOrder], army: ArmyId) -> bool {
    moves.iter().any(|m| m.army == army)
}

fn moves_to(moves: &[MoveOrder], region: RegionId) -> u32 {
    let n = moves
        .iter()
        .filter(|m| !m.heroless && matches!(m.kind, MoveKind::Move { region: r } if r == region))
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

struct Path {
    nodes: Vec<RegionId>,
    cost: Vec<u32>,
}

fn path(ctx: &Ctx<'_>, from: RegionId, to: RegionId) -> Option<Path> {
    let state = ctx.state;
    let n = state.regions.len();
    let mut dist: Vec<Option<u32>> = vec![None; n];
    let mut prev: Vec<Option<usize>> = vec![None; n];
    let mut done = vec![false; n];
    *dist.get_mut(from.index())? = Some(0);
    let passable = |r: RegionId| {
        state.region(r).is_some_and(|s| {
            s.enabled && (s.owner.is_none() || state.allied_owner(s.owner, ctx.me))
        })
    };
    loop {
        let next = (0..n)
            .filter(|i| !done.get(*i).copied().unwrap_or(true))
            .filter_map(|i| Some((i, dist.get(i).copied().flatten()?)))
            .min_by_key(|(i, d)| (*d, *i));
        let Some((u, du)) = next else { break };
        if let Some(flag) = done.get_mut(u) {
            *flag = true;
        }
        let ur = RegionId(u16::try_from(u).ok()?);
        if ur == to || (ur != from && !passable(ur)) {
            continue;
        }
        for v in &state.rules.region_rules(ur)?.neighbours {
            let step = u32::from(state.step_cost(ur, *v));
            let cand = du.saturating_add(step);
            if dist
                .get(v.index())
                .copied()
                .flatten()
                .is_none_or(|old| cand < old)
            {
                *dist.get_mut(v.index())? = Some(cand);
                *prev.get_mut(v.index())? = Some(u);
            }
        }
    }
    dist.get(to.index()).copied().flatten()?;
    let mut nodes = Vec::new();
    let mut at = Some(to.index());
    while let Some(i) = at {
        nodes.push(RegionId(u16::try_from(i).ok()?));
        at = prev.get(i).copied().flatten();
        if nodes.len() > n {
            return None;
        }
    }
    nodes.reverse();
    let cost = nodes
        .iter()
        .map(|r| dist.get(r.index()).copied().flatten().unwrap_or(0))
        .collect();
    Some(Path { nodes, cost })
}

fn step_toward(ctx: &Ctx<'_>, from: RegionId, to: RegionId) -> Option<RegionId> {
    let p = path(ctx, from, to)?;
    match p.nodes.len() {
        0 | 1 => None,
        2 => p.nodes.get(1).copied(),
        _ => {
            let second = p.nodes.get(2).copied()?;
            if p.cost.get(2).copied().unwrap_or(u32::MAX) <= MAX_PATH_STEP_COST {
                Some(second)
            } else {
                p.nodes.get(1).copied()
            }
        }
    }
}

fn push_move(moves: &mut Vec<MoveOrder>, army: ArmyId, region: RegionId) {
    moves.push(MoveOrder {
        army,
        heroless: false,
        kind: MoveKind::Move { region },
    });
}

pub(super) fn capital_defence(ctx: &mut Ctx<'_>, moves: &mut Vec<MoveOrder>) {
    let Some(player) = ctx.state.player(ctx.me) else {
        return;
    };
    let wanted = ctx
        .state
        .rules
        .conditions
        .player_defeat
        .iter()
        .any(|c| c.lose_if_capital_lost && names_team(&c.teams, player.team));
    let Some(capital) = player.base_region.filter(|_| wanted) else {
        return;
    };
    let e = ctx
        .snap
        .nearest(capital, false, |a| a.hero && !is_ally(ctx, a.player))
        .map_or(FAR, |(_, d)| d);
    let Some((h, n)) = ctx
        .snap
        .nearest(capital, true, |a| a.hero && is_ally(ctx, a.player))
    else {
        return;
    };
    if !(u32::from(e) < u32::from(n).saturating_add(1) || e == 1) {
        return;
    }
    if has_move(moves, h.id) {
        return;
    }
    if n == 0 {
        push_move(moves, h.id, capital);
    } else if let Some(step) = step_toward(ctx, h.region, capital) {
        push_move(moves, h.id, step);
    }
}

fn scale_for(difficulty: u8, simulation: u8) -> Option<Scale> {
    match (difficulty, simulation) {
        (3, _) => Some(Scale {
            weapon: 0.9,
            armor: 1.1,
        }),
        (2, 1) => Some(Scale {
            weapon: 0.75,
            armor: 1.25,
        }),
        (2, _) => Some(Scale {
            weapon: 0.5,
            armor: 1.5,
        }),
        _ => None,
    }
}

fn spec_side(ctx: &Ctx<'_>, armies: &[ArmyId]) -> SpecSide {
    let mut players: Vec<PlayerId> = Vec::new();
    let mut list: Vec<Army> = Vec::new();
    for id in armies {
        if let Some(a) = ctx.state.armies.get(id) {
            if !players.contains(&a.player) {
                players.push(a.player);
            }
            list.push(a.clone());
        }
    }
    SpecSide {
        players,
        armies: list,
    }
}

pub(crate) fn predict(
    ctx: &mut Ctx<'_>,
    region: RegionId,
    mine: &[ArmyId],
    theirs: &[ArmyId],
    scale: Option<Scale>,
) -> (Prediction, u8) {
    let owner = ctx.state.owner(region);
    let ai_first = owner.is_none_or(|o| is_ally(ctx, o));
    let (mine_side, their_side) = (spec_side(ctx, mine), spec_side(ctx, theirs));
    let (sides, ai_side) = if ai_first {
        (vec![mine_side, their_side], 0u8)
    } else {
        (vec![their_side, mine_side], 1u8)
    };
    let spec = BattleSpec {
        battle: 0,
        region,
        owner,
        sides,
        map: None,
        region_name: String::new(),
        neighbours: Vec::new(),
        participants: Vec::new(),
        seed: 0,
        weapon_scale: Vec::new(),
        round_cap: None,
        revival_multipliers: (1.0, 1.0),
    };
    let outcome = ctx
        .predictor
        .predict(&spec, ai_side, scale, ctx.rolls.rng_mut());
    (outcome, ai_side)
}

fn simulate(
    ctx: &mut Ctx<'_>,
    region: RegionId,
    mine: &[ArmyId],
    theirs: &[ArmyId],
    simulation: u8,
) -> bool {
    let scale = scale_for(ctx.difficulty, simulation);
    let (outcome, ai_side) = predict(ctx, region, mine, theirs, scale);
    outcome.winner == Some(ai_side)
}

pub(crate) fn outlook(ctx: &mut Ctx<'_>, army: ArmyId, region: RegionId) -> u8 {
    let k = match ctx.difficulty {
        2 => 0,
        3 => 1,
        _ => 2,
    };
    let mut enemies: Vec<ArmyId> = Vec::new();
    let mut allies: Vec<ArmyId> = Vec::new();
    let snap = ctx.snap;
    for hop in 0..=SEEN_RADIUS {
        for r in snap.ring(region, hop) {
            for a in snap.armies().filter(|a| a.region == *r) {
                if is_ally(ctx, a.player) {
                    if a.id != army {
                        allies.push(a.id);
                    }
                } else if hop == 0 || ctx.rolls.int(0, k) == 0 {
                    enemies.push(a.id);
                }
            }
        }
    }
    if enemies.is_empty() {
        return 0;
    }
    if simulate(ctx, region, &[army], &enemies, 1) {
        return 0;
    }
    if !allies.is_empty() {
        let mut side = allies;
        side.push(army);
        if simulate(ctx, region, &side, &enemies, 2) {
            return 1;
        }
    }
    2
}

fn territory(ctx: &mut Ctx<'_>, moves: &mut Vec<MoveOrder>) -> Option<RegionId> {
    let pref = ctx.template.preference;
    let at = |i: usize| pref.get(i).copied().unwrap_or(10) as f32;
    let mut best: Option<(usize, f32)> = None;
    for (index, t) in ctx.state.rules.territories.iter().enumerate() {
        let held = t
            .regions
            .iter()
            .filter(|r| ctx.state.owner(**r).is_some_and(|o| is_ally(ctx, o)))
            .count();
        if held >= t.regions.len() {
            continue;
        }
        let b = |f: usize| t.bonus.get(f) as f32;
        let score = 0.01
            * (b(bonus::LEGENDARY) * at(2)
                + b(bonus::ATTACK) * at(3)
                + b(bonus::DEFENSE) * at(4)
                + b(bonus::EXPERIENCE) * at(5))
            + held as f32 / t.regions.len() as f32;
        if score > 0.0 && best.is_none_or(|(_, s)| score > s) {
            best = Some((index, score));
        }
    }
    let (index, _) = best?;
    let regions = ctx.state.rules.territories.get(index)?.regions.clone();
    let heroes = ctx.snap.summary(ctx.me)?.hero_armies.clone();
    let mut pick: Option<(ArmyId, RegionId, f32)> = None;
    for army in heroes {
        if has_move(moves, army) {
            continue;
        }
        let Some(rec) = ctx.snap.army(army) else {
            continue;
        };
        for j in &regions {
            if ctx.state.owner(*j) == Some(ctx.me) {
                continue;
            }
            let d = f32::from(ctx.snap.dist_or_far(rec.region, *j));
            let mut s = 1.0 / (d * count_f(rec.members.len()));
            let enemy_owned = ctx.state.owner(*j).is_some_and(|o| !is_ally(ctx, o));
            let enabled = ctx.state.region(*j).is_some_and(|r| r.enabled);
            if !(enabled && enemy_owned && ctx.state.armies.contains_key(&army))
                || outlook(ctx, army, *j) != 0
            {
                s = 0.0;
            }
            if s > 0.0 && pick.is_none_or(|(_, _, b)| s > b) {
                pick = Some((army, *j, s));
            }
        }
    }
    let (army, target, _) = pick?;
    let from = ctx.snap.army(army)?.region;
    if let Some(step) = step_toward(ctx, from, target) {
        push_move(moves, army, step);
    }
    Some(target)
}

fn count_f(n: usize) -> f32 {
    f32::from(u16::try_from(n).unwrap_or(u16::MAX))
}

fn neighbour_score(
    ctx: &mut Ctx<'_>,
    army: ArmyId,
    home: RegionId,
    n: RegionId,
    moves: &[MoveOrder],
    target: Option<RegionId>,
    enemy_region: Option<RegionId>,
) -> i32 {
    let state = ctx.state;
    let Some(region) = state.region(n).filter(|r| r.enabled) else {
        return 0;
    };
    let Some(rec) = ctx.snap.region(n) else {
        return 0;
    };
    let m = outlook(ctx, army, n);
    let rules = state.rules.region_rules(n);
    let spots = i32::try_from(rules.map_or(0, |r| r.plots)).unwrap_or(0);
    let flag = |on: bool, by: i32| if on { by } else { 0 };
    let fort = rules.is_some_and(|r| r.create_auto_fort);
    let army_bonus = rules.is_some_and(|r| r.bonus.get(bonus::ARMY) > 0);
    let fertile = rules.is_some_and(|r| r.bonus.get(bonus::FERTILE_TERRITORY) > 0);
    let sv = rec.strategic_value;
    let nearer = |goal: Option<RegionId>| {
        goal.is_some_and(|g| ctx.snap.dist_or_far(n, g) < ctx.snap.dist_or_far(home, g))
    };
    let (near_enemy, near_target) = (nearer(enemy_region), nearer(target));
    let mut score = match region.owner {
        None => sv
            .saturating_add(30)
            .saturating_add(flag(near_enemy, 10))
            .saturating_add(flag(near_target, 15))
            .saturating_add(flag(m != 2, 10))
            .saturating_add(spots.saturating_mul(5))
            .saturating_sub(5)
            .saturating_add(flag(fort, 25))
            .saturating_add(flag(army_bonus, 10))
            .saturating_add(flag(fertile, 10)),
        Some(o) if is_ally(ctx, o) => {
            let upgrades = i32::try_from(rec.upgrade.len()).unwrap_or(0);
            let mut s = sv
                .saturating_add(upgrades.saturating_mul(3))
                .saturating_add(5)
                .saturating_add(flag(near_enemy, 10))
                .saturating_add(flag(near_target, 15))
                .saturating_add(spots.saturating_mul(2))
                .saturating_sub(2)
                .saturating_add(flag(fort, 10))
                .saturating_add(flag(army_bonus, 5))
                .saturating_add(flag(fertile, 5));
            let threatened = ctx
                .snap
                .nearest(n, false, |a| a.hero && !is_ally(ctx, a.player))
                .is_some_and(|(_, d)| d <= 2);
            if threatened {
                let fortresses = i32::try_from(rec.strengthen.len()).unwrap_or(0);
                s = s
                    .saturating_add(flag(m != 2, 10))
                    .saturating_add(fortresses.saturating_mul(5));
            }
            s
        }
        Some(_) => {
            let mut s = sv
                .saturating_add(flag(near_target, 15))
                .saturating_add(spots.saturating_mul(5))
                .saturating_sub(5)
                .saturating_add(flag(fort, 25))
                .saturating_add(flag(army_bonus, 10))
                .saturating_add(flag(fertile, 10));
            match m {
                0 => s = s.saturating_add(40),
                1 => s = s.saturating_add(25),
                _ => s = s.saturating_sub(10).max(0),
            }
            s
        }
    };
    if m != 1 {
        score >>= moves_to(moves, n).min(31);
    }
    score
}

pub(crate) fn plan(ctx: &mut Ctx<'_>, moves: &mut Vec<MoveOrder>) {
    capital_defence(ctx, moves);
    let mut target = None;
    if ctx.difficulty > 2 && ctx.rolls.int(0, 1) == 0 {
        target = territory(ctx, moves);
    }
    let heroes = ctx
        .snap
        .summary(ctx.me)
        .map(|s| s.hero_armies.clone())
        .unwrap_or_default();
    for army in heroes {
        if has_move(moves, army) {
            continue;
        }
        let Some(rec) = ctx.snap.army(army) else {
            continue;
        };
        let home = rec.region;
        let Some(here) = ctx.snap.region(home) else {
            continue;
        };
        let mut stay = here.strategic_value.saturating_add(
            i32::try_from(here.upgrade.len())
                .unwrap_or(0)
                .saturating_mul(3),
        );
        if ctx
            .snap
            .nearest(home, false, |a| a.hero && !is_ally(ctx, a.player))
            .is_some_and(|(_, d)| d <= 1)
        {
            stay = stay
                .saturating_add(
                    i32::try_from(here.strengthen.len())
                        .unwrap_or(0)
                        .saturating_mul(5),
                )
                .saturating_add(5);
        }
        let enemy_region = ctx
            .snap
            .nearest_region(home, |r| r.owner.is_some_and(|o| !is_ally(ctx, o)));
        let neighbours = ctx
            .state
            .rules
            .region_rules(home)
            .map(|r| r.neighbours.clone())
            .unwrap_or_default();
        let mut best: Option<(RegionId, i32)> = None;
        for n in neighbours {
            let s = neighbour_score(ctx, army, home, n, moves, target, enemy_region);
            if s > stay && best.is_none_or(|(_, b)| s > b) {
                best = Some((n, s));
            }
        }
        if let Some((n, _)) = best {
            push_move(moves, army, n);
        }
    }
}
