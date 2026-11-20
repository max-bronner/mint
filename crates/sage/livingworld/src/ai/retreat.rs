use crate::ai::hero::predict;
use crate::ai::Ctx;
use crate::model::{ArmyId, RegionId};

const HOLD_ROUNDS: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleAnswer {
    Undecided = 0,
    AutoResolve = 1,
    Withdraw = 2,
    RealTime = 3,
    Abstain = 4,
}

pub(crate) fn retreat_choice(ctx: &mut Ctx<'_>, army: ArmyId) -> Option<RegionId> {
    let a = ctx.state.armies.get(&army)?;
    let options: Vec<RegionId> = ctx
        .state
        .rules
        .region_rules(a.region)?
        .neighbours
        .iter()
        .copied()
        .filter(|n| ctx.state.allied_owner(ctx.state.owner(*n), ctx.me))
        .collect();
    if options.is_empty() {
        return ctx.state.player(ctx.me)?.base_region;
    }
    options.get(ctx.rolls.pick(options.len())).copied()
}

pub(crate) fn battle_answer(ctx: &mut Ctx<'_>, region: RegionId) -> BattleAnswer {
    let hero_army = ctx
        .state
        .armies
        .values()
        .any(|a| a.region == region && a.player == ctx.me && a.is_hero_army());
    if !hero_army {
        return BattleAnswer::Abstain;
    }
    let capital = ctx.state.capital_decides(ctx.me, region);
    let ally_next_door = ctx.state.rules.region_rules(region).is_some_and(|r| {
        r.neighbours
            .iter()
            .any(|n| ctx.state.allied_owner(ctx.state.owner(*n), ctx.me))
    });
    if !ally_next_door || capital {
        return BattleAnswer::Abstain;
    }
    let (mut mine, mut theirs) = (Vec::new(), Vec::new());
    for a in ctx.state.armies.values().filter(|a| a.region == region) {
        if ctx.state.allied(ctx.me, a.player) {
            mine.push(a.id);
        } else {
            theirs.push(a.id);
        }
    }
    let (outcome, ai_side) = predict(ctx, region, &mine, &theirs, None);
    if outcome.winner != Some(ai_side) && outcome.rounds < HOLD_ROUNDS {
        BattleAnswer::Withdraw
    } else {
        BattleAnswer::Abstain
    }
}
