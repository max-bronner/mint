use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::masks::PlayerMask;
use super::{
    compare, parameter_int, parameter_string, ActionVerb, ConditionVerb, MapScriptEngine,
    MapScriptState, ParseCtx,
};
use crate::rules::{Ids, RotwkRules};
use crate::sciences::ScienceId;

fn availability_withholds(text: &str) -> Option<bool> {
    let text = text.trim();
    [("available", false), ("disabled", true), ("hidden", true)]
        .into_iter()
        .find(|(name, _)| text.eq_ignore_ascii_case(name))
        .map(|(_, withheld)| withheld)
}

enum Verb {
    GrantScience {
        science: String,
    },
    ScienceAvailability {
        science: String,
        withheld: Option<bool>,
    },
    GiveMoney {
        amount: i32,
    },
    GiveUpgrade {
        upgrade: String,
    },
    SetUnitConstruction {
        enabled: bool,
    },
    SetBaseConstruction {
        enabled: bool,
    },
}

struct ActionPlayer {
    player_text: String,
    verb: Verb,
}

impl ActionVerb for ActionPlayer {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        for target in mask.players() {
            match &self.verb {
                Verb::GrantScience { science } => {
                    rules.script_grant_science(tick, ids, target, science)?;
                }
                Verb::ScienceAvailability { science, withheld } => {
                    if let (Some(id), Some(withheld)) = (rules.sciences.id(science), *withheld) {
                        rules.script_set_science_availability(tick, ids, target, id, withheld)?;
                    }
                }
                Verb::GiveMoney { amount } => {
                    rules.script_give_money(tick, ids, target, *amount)?;
                }
                Verb::GiveUpgrade { upgrade } => {
                    if let Some(bit) = rules.upgrade_bits.bit(upgrade) {
                        if rules.upgrade_bits.is_player(bit) {
                            rules.grant_player_upgrade(tick, ids, target, bit)?;
                        }
                    }
                }
                Verb::SetUnitConstruction { enabled } => {
                    rules.script_set_build_switch(tick, ids, target, Some(*enabled), None)?;
                }
                Verb::SetBaseConstruction { enabled } => {
                    rules.script_set_build_switch(tick, ids, target, None, Some(*enabled))?;
                }
            }
        }
        Ok(())
    }
}

impl RotwkRules {
    fn script_grant_science(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        science: &str,
    ) -> Result<()> {
        let Some(id) = self.sciences.id(science) else {
            return Ok(());
        };
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        let Some(mut held) = tick.state().get(ids.player_science, entity).cloned() else {
            return Ok(());
        };
        if held.add(id) {
            tick.state_mut().insert(ids.player_science, entity, held)?;
            self.sciences_acquired(tick, ids, player, &[id])?;
        }
        Ok(())
    }

    fn script_set_science_availability(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        science: ScienceId,
        withheld: bool,
    ) -> Result<()> {
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        if let Some(mut held) = tick.state().get(ids.player_science, entity).cloned() {
            held.set_unavailable(science, withheld);
            tick.state_mut().insert(ids.player_science, entity, held)?;
        }
        Ok(())
    }

    fn script_set_build_switch(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        units: Option<bool>,
        base: Option<bool>,
    ) -> Result<()> {
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        let mut switches = tick
            .state()
            .get(ids.build_switches, entity)
            .copied()
            .unwrap_or(crate::economy::BuildSwitches {
                can_build_units: true,
                can_build_base: true,
            });
        switches.can_build_units = units.unwrap_or(switches.can_build_units);
        switches.can_build_base = base.unwrap_or(switches.can_build_base);
        tick.state_mut()
            .insert(ids.build_switches, entity, switches)?;
        crate::diag::diag!(
            "script: build switches of player {} are units {}, base {}",
            player.get(),
            switches.can_build_units,
            switches.can_build_base
        );
        Ok(())
    }

    pub(super) fn script_give_money(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        amount: i32,
    ) -> Result<()> {
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        let Some(mut purse) = tick.state().get(ids.purse, entity).copied() else {
            return Ok(());
        };
        if amount < 0 {
            purse.0.withdraw(i64::from(amount).saturating_neg());
        } else {
            purse.0.deposit(i64::from(amount));
        }
        tick.state_mut().insert(ids.purse, entity, purse)?;
        Ok(())
    }
}

enum Check {
    OwnedBy { unit: String },
    HasCredits { credits: i32, cmp: i32 },
    AcquiredScience { science: String },
    Faction { faction: String },
    ObjectComparison { cmp: i32, count: i32, types: String },
    NamedDiscovered { unit: String },
    TeamDiscovered { team: String },
}

struct ConditionPlayer {
    player_text: String,
    side: String,
    check: Check,
}

impl ConditionPlayer {
    fn seen_by(
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        mask: PlayerMask,
        entity: Entity,
    ) -> bool {
        use mint_sage_shroud::ObjectVisibility::{Clear, PartialClear};
        let world = tick.state();
        let seen = rules.horde_members(tick, ids, &[entity]);
        mask.any(|p| {
            seen.iter().any(|&object| {
                rules.sees(world, ids, p, object)
                    && world
                        .get(ids.seen, object)
                        .is_some_and(|seen| matches!(seen.visibility(p), Clear | PartialClear))
            })
        })
    }
}

impl ConditionVerb for ConditionPlayer {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        let world = tick.state();
        match &self.check {
            Check::OwnedBy { unit } => rules
                .object_named(tick, ids, &state.rt, unit, &self.side)
                .and_then(|entity| rules.owner_of(tick, ids, entity))
                .is_some_and(|owner| mask.contains(owner)),
            Check::HasCredits { credits, cmp } => {
                let money = mask.sum(|p| {
                    crate::economy::purse_of(world, RotwkRules::economy_ids(ids), p).amount()
                });
                let money = i32::try_from(money).unwrap_or(i32::MAX);
                compare(*credits, *cmp, money)
            }
            Check::AcquiredScience { science } => rules
                .sciences
                .id(science)
                .is_some_and(|_| mask.any(|p| rules.holds_science(world, ids, p, science))),
            Check::Faction { faction } => mask.any(|p| {
                rules
                    .player_sides()
                    .iter()
                    .any(|(held, side)| *held == p && side.eq_ignore_ascii_case(faction))
            }),
            Check::ObjectComparison { cmp, count, types } => {
                let names = state.rt.lists.types_of(types);
                let total = mask.sum(|p| rules.count_objects_of(tick, ids, p, &names));
                compare(i32::try_from(total).unwrap_or(i32::MAX), *cmp, *count)
            }
            Check::NamedDiscovered { unit } => rules
                .object_named(tick, ids, &state.rt, unit, &self.side)
                .is_some_and(|entity| Self::seen_by(rules, tick, ids, mask, entity)),
            Check::TeamDiscovered { team } => {
                rules
                    .peek_team(state, team, &self.side)
                    .is_some_and(|team| {
                        rules
                            .live_members(tick, state, team)
                            .into_iter()
                            .any(|member| Self::seen_by(rules, tick, ids, mask, member))
                    })
            }
        }
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        match &self.check {
            Check::OwnedBy { unit } | Check::NamedDiscovered { unit } => Some(unit),
            _ => None,
        }
    }

    fn team_names(&self) -> Vec<String> {
        match &self.check {
            Check::TeamDiscovered { team } => vec![team.clone()],
            _ => Vec::new(),
        }
    }
}

impl RotwkRules {
    pub(super) fn count_objects_of(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        names: &[String],
    ) -> i64 {
        let world = tick.state();
        let Some(store) = world.store(ids.kind) else {
            return 0;
        };
        let count = store
            .iter()
            .filter(|(entity, _)| {
                world.is_live(*entity)
                    && !world.contains(ids.doomed, *entity)
                    && world.get(ids.owner, *entity).is_some_and(|o| o.0 == player)
            })
            .filter_map(|(_, kind)| self.kinds.get(*kind))
            .filter(|entry| !entry.record.has_kind("INERT"))
            .filter(|entry| {
                names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&entry.record.name))
            })
            .count();
        i64::try_from(count).unwrap_or(i64::MAX)
    }
}

fn player_verb(params: &[Parameter], verb: Verb) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionPlayer {
        player_text: parameter_string(params, 0)?,
        verb,
    }))
}

pub(super) fn parse_grant_science(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    player_verb(
        params,
        Verb::GrantScience {
            science: parameter_string(params, 1)?,
        },
    )
}

pub(super) fn parse_science_availability(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    player_verb(
        params,
        Verb::ScienceAvailability {
            science: parameter_string(params, 1)?,
            withheld: availability_withholds(&parameter_string(params, 2)?),
        },
    )
}

pub(super) fn parse_give_money(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    player_verb(
        params,
        Verb::GiveMoney {
            amount: parameter_int(params, 1)?,
        },
    )
}

pub(super) fn parse_give_upgrade(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    player_verb(
        params,
        Verb::GiveUpgrade {
            upgrade: parameter_string(params, 1)?,
        },
    )
}

pub(super) fn parse_enable_unit_construction(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    player_verb(params, Verb::SetUnitConstruction { enabled: true })
}

pub(super) fn parse_disable_unit_construction(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    player_verb(params, Verb::SetUnitConstruction { enabled: false })
}

pub(super) fn parse_enable_base_construction(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    player_verb(params, Verb::SetBaseConstruction { enabled: true })
}

pub(super) fn parse_disable_base_construction(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    player_verb(params, Verb::SetBaseConstruction { enabled: false })
}

fn condition(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    player_index: usize,
    check: Check,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionPlayer {
        player_text: parameter_string(params, player_index).unwrap_or_default(),
        side: ctx.side.to_owned(),
        check,
    }))
}

pub(super) fn parse_named_owned_by_player(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let unit = parameter_string(params, 0)?;
    condition(params, ctx, 1, Check::OwnedBy { unit })
}

pub(super) fn parse_player_has_credits(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let check = Check::HasCredits {
        credits: parameter_int(params, 0)?,
        cmp: parameter_int(params, 1)?,
    };
    condition(params, ctx, 2, check)
}

pub(super) fn parse_player_acquired_science(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let science = parameter_string(params, 1)?;
    condition(params, ctx, 0, Check::AcquiredScience { science })
}

pub(super) fn parse_skirmish_player_faction(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let faction = parameter_string(params, 1)?;
    condition(params, ctx, 0, Check::Faction { faction })
}

pub(super) fn parse_player_has_object_comparison(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let check = Check::ObjectComparison {
        cmp: parameter_int(params, 1)?,
        count: parameter_int(params, 2)?,
        types: parameter_string(params, 3)?,
    };
    condition(params, ctx, 0, check)
}

pub(super) fn parse_named_discovered(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let unit = parameter_string(params, 0)?;
    condition(params, ctx, 1, Check::NamedDiscovered { unit })
}

pub(super) fn parse_team_discovered(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let team = parameter_string(params, 0)?;
    condition(params, ctx, 1, Check::TeamDiscovered { team })
}
