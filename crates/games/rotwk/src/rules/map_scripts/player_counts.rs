use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick, World};

use super::count_cache::CacheKey;
use super::counters::SideKey;
use super::masks::PlayerMask;
use super::{
    compare, parameter_int, parameter_string, side_key, ActionVerb, ConditionVerb, MapScriptEngine,
    MapScriptState, ParseCtx,
};
use crate::rules::{Ids, RotwkRules};
use crate::unit::Kind;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Census {
    Living,
    WithDead,
}

impl RotwkRules {
    fn type_kinds(&self, state: &MapScriptState, name: &str) -> Vec<Kind> {
        state
            .rt
            .lists
            .types_of(name)
            .iter()
            .filter_map(|template| self.kinds.kind(template))
            .collect()
    }

    fn census(&self, world: &World, ids: Ids, player: PlayerId, kinds: &[Kind], by: Census) -> i64 {
        let Some(store) = world.store(ids.owner) else {
            return 0;
        };
        let count = store
            .iter()
            .filter(|(entity, owner)| {
                owner.0 == player
                    && world.is_live(*entity)
                    && !world.contains(ids.construction, *entity)
                    && (by == Census::WithDead || !world.contains(ids.doomed, *entity))
                    && world
                        .get(ids.kind, *entity)
                        .is_some_and(|kind| kinds.contains(kind))
            })
            .count();
        i64::try_from(count).unwrap_or(i64::MAX)
    }

    pub(super) fn sync_count_stamp(&self, world: &World, ids: Ids, state: &mut MapScriptState) {
        if let Some((_, stored)) = world
            .store(ids.map_script_state)
            .and_then(|store| store.iter().next())
        {
            state.rt.counts.adopt_later_stamp(&stored.rt.counts);
        }
    }

    fn write_counter(state: &mut MapScriptState, key: &SideKey, value: i64) {
        state.counters.entry(key.clone()).or_default().value =
            i32::try_from(value).unwrap_or(i32::MAX);
    }

    pub(super) fn owned_by(world: &World, ids: Ids, player: PlayerId) -> Vec<Entity> {
        world.store(ids.owner).map_or_else(Vec::new, |store| {
            store
                .iter()
                .filter(|(entity, owner)| owner.0 == player && world.is_live(*entity))
                .map(|(entity, _)| entity)
                .collect()
        })
    }

    fn kills_of_kind(world: &World, ids: Ids, player: PlayerId, kind: Kind) -> u32 {
        crate::economy::player_entity(world, Self::economy_ids(ids), player)
            .and_then(|entity| world.get(ids.match_stats, entity))
            .map_or(0, |stats| stats.kills_of(kind.0))
    }
}

enum Count {
    KillsOfType { template: String },
    CommandPointsUsed,
    CommandPointsTotal,
    Ownership { types: String, census: Census },
}

struct ActionCount {
    player: String,
    counter: SideKey,
    count: Count,
    cache: CacheKey,
}

impl ActionVerb for ActionCount {
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
        let mask = rules.player_mask(&state.rt, &self.player, current);
        let world = tick.state();
        match &self.count {
            Count::KillsOfType { template } => {
                let Some(player) = mask.single() else {
                    return Ok(());
                };
                let kills = rules.kinds.kind(template).map_or(0, |kind| {
                    RotwkRules::kills_of_kind(world, ids, player, kind)
                });
                RotwkRules::write_counter(state, &self.counter, i64::from(kills));
            }
            Count::CommandPointsUsed | Count::CommandPointsTotal => {
                let Some(player) = mask.single() else {
                    return Ok(());
                };
                let points = crate::economy::points_of(world, RotwkRules::economy_ids(ids), player);
                let value = if matches!(self.count, Count::CommandPointsUsed) {
                    points.used()
                } else {
                    points.limit()
                };
                RotwkRules::write_counter(state, &self.counter, i64::from(value));
            }
            Count::Ownership { types, census } => {
                rules.sync_count_stamp(world, ids, state);
                if state.rt.counts.action_is_current(self.cache) {
                    return Ok(());
                }
                let kinds = rules.type_kinds(state, types);
                let total = mask.sum(|player| rules.census(world, ids, player, &kinds, *census));
                RotwkRules::write_counter(state, &self.counter, total);
                state.rt.counts.mark_action_ran(self.cache);
            }
        }
        Ok(())
    }
}

enum Check {
    BuiltBy {
        template: String,
        cache: CacheKey,
    },
    NumWithUpgrade {
        count: i32,
        upgrade: String,
    },
    PurchasePoints {
        points: i32,
    },
    ModelCondition {
        condition: String,
        cmp: i32,
        count: i32,
    },
    Garrisoned {
        cmp: i32,
        count: i32,
    },
    KilledTypeUnits {
        count: i32,
        types: String,
    },
}

struct ConditionCount {
    player: String,
    check: Check,
}

impl ConditionVerb for ConditionCount {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        current: Option<PlayerId>,
    ) -> bool {
        let mask = rules.player_mask(&state.rt, &self.player, current);
        self.holds(rules, tick.state(), ids, state, mask)
    }

    fn eval_mut(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        current: Option<PlayerId>,
    ) -> bool {
        let Check::BuiltBy { cache, .. } = &self.check else {
            return self.eval(rules, tick, ids, state, current);
        };
        rules.sync_count_stamp(tick.state(), ids, state);
        if let Some(result) = state.rt.counts.fresh(*cache) {
            return result > 0;
        }
        let held = self.eval(rules, tick, ids, state, current);
        state.rt.counts.store(*cache, if held { 1 } else { -1 });
        held
    }
}

impl ConditionCount {
    fn holds(
        &self,
        rules: &RotwkRules,
        world: &World,
        ids: Ids,
        state: &MapScriptState,
        mask: PlayerMask,
    ) -> bool {
        match &self.check {
            Check::BuiltBy { template, .. } => {
                if state.rt.lists.is_list(template) {
                    return false;
                }
                let Some(kind) = rules.kinds.kind(template) else {
                    return false;
                };
                mask.any(|player| rules.census(world, ids, player, &[kind], Census::WithDead) > 0)
            }
            Check::NumWithUpgrade { count, upgrade } => {
                let Some(player) = mask.single() else {
                    return false;
                };
                let Some(bit) = rules.upgrade_bits.bit(upgrade) else {
                    return false;
                };
                let held = RotwkRules::owned_by(world, ids, player)
                    .into_iter()
                    .filter(|&entity| !world.contains(ids.member, entity))
                    .filter(|&entity| rules.holds_object_upgrade(world, ids, entity, bit))
                    .count();
                i32::try_from(held).unwrap_or(i32::MAX) >= *count
            }
            Check::PurchasePoints { points } => mask.any(|player| {
                i64::from(rules.purchase_points_of(world, player)) >= i64::from(*points)
            }),
            Check::ModelCondition {
                condition,
                cmp,
                count,
            } => {
                let total = mask.sum(|player| {
                    let held = RotwkRules::owned_by(world, ids, player)
                        .into_iter()
                        .filter(|&entity| rules.holds_condition_in(world, entity, condition))
                        .count();
                    i64::try_from(held).unwrap_or(i64::MAX)
                });
                compare(i32::try_from(total).unwrap_or(i32::MAX), *cmp, *count)
            }
            Check::Garrisoned { cmp, count } => {
                let Some(player) = mask.first() else {
                    return false;
                };
                let held = RotwkRules::owned_by(world, ids, player)
                    .into_iter()
                    .filter(|&entity| {
                        rules
                            .contain_view(world, entity)
                            .is_some_and(|view| view.garrison_family)
                            && world
                                .get(ids.passengers, entity)
                                .is_some_and(|held| !held.0.is_empty())
                    })
                    .count();
                compare(i32::try_from(held).unwrap_or(i32::MAX), *cmp, *count)
            }
            Check::KilledTypeUnits { count, types } => {
                let Some(player) = mask.first() else {
                    return false;
                };
                let killed: u64 = rules
                    .type_kinds(state, types)
                    .into_iter()
                    .map(|kind| u64::from(RotwkRules::kills_of_kind(world, ids, player, kind)))
                    .sum();
                i64::try_from(killed).unwrap_or(i64::MAX) >= i64::from(*count)
            }
        }
    }
}

impl RotwkRules {
    fn holds_object_upgrade(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        bit: mint_sim::UpgradeBit,
    ) -> bool {
        world
            .get(ids.object_upgrades, entity)
            .is_some_and(|mask| mask.0.contains(bit))
            || self
                .level_upgrades(world, ids, entity)
                .is_some_and(|granted| granted.contains(bit))
            || self
                .forwarded_level_upgrades(world, ids, entity)
                .is_some_and(|granted| granted.contains(bit))
    }
}

fn counted(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    player: usize,
    counter: usize,
    count: Count,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionCount {
        player: parameter_string(params, player)?,
        counter: side_key(params, counter, ctx)?,
        count,
        cache: ctx.next_cache_key(),
    }))
}

pub(super) fn parse_kills_of_type(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let template = parameter_string(params, 1)?;
    counted(params, ctx, 0, 2, Count::KillsOfType { template })
}

pub(super) fn parse_command_points_used(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    counted(params, ctx, 0, 1, Count::CommandPointsUsed)
}

pub(super) fn parse_command_points_total(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    counted(params, ctx, 0, 1, Count::CommandPointsTotal)
}

pub(super) fn parse_ownership_counter(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let types = parameter_string(params, 0)?;
    counted(
        params,
        ctx,
        1,
        2,
        Count::Ownership {
            types,
            census: Census::Living,
        },
    )
}

pub(super) fn parse_ownership_counter_with_dead(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let types = parameter_string(params, 0)?;
    counted(
        params,
        ctx,
        1,
        2,
        Count::Ownership {
            types,
            census: Census::WithDead,
        },
    )
}

fn condition(params: &[Parameter], player: usize, check: Check) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionCount {
        player: parameter_string(params, player)?,
        check,
    }))
}

pub(super) fn parse_built_by_player(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let template = parameter_string(params, 0)?;
    condition(
        params,
        1,
        Check::BuiltBy {
            template,
            cache: ctx.next_cache_key(),
        },
    )
}

pub(super) fn parse_num_units_with_upgrade(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let check = Check::NumWithUpgrade {
        count: parameter_int(params, 1)?,
        upgrade: parameter_string(params, 2)?,
    };
    condition(params, 0, check)
}

pub(super) fn parse_science_purchase_points(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let check = Check::PurchasePoints {
        points: parameter_int(params, 1)?,
    };
    condition(params, 0, check)
}

pub(super) fn parse_objects_with_model_condition(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let check = Check::ModelCondition {
        condition: parameter_string(params, 1)?,
        cmp: parameter_int(params, 2)?,
        count: parameter_int(params, 3)?,
    };
    condition(params, 0, check)
}

pub(super) fn parse_comparison_garrisoned(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let check = Check::Garrisoned {
        cmp: parameter_int(params, 1)?,
        count: parameter_int(params, 2)?,
    };
    condition(params, 0, check)
}

pub(super) fn parse_has_killed_type_units(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let check = Check::KilledTypeUnits {
        count: parameter_int(params, 1)?,
        types: parameter_string(params, 2)?,
    };
    condition(params, 0, check)
}
