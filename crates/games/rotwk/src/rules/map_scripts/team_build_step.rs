use mint_sim::{Entity, PlayerId, Result, Tick, World};

use super::counters::TICKS_PER_SCRIPT_FRAME;
use super::status::ScriptStatusBit;
use super::team::TeamId;
use super::team_build::{orders_for, QueuedTeam};
use super::{MapScriptEngine, MapScriptState};
use crate::diag::diag;
use crate::game_data::RETAIL_LOGIC_HZ;
use crate::lobby::Difficulty;
use crate::rules::{Ids, RotwkRules};
use crate::unit::Kind;

const ALWAYS_TRUE: &str = "<!TRUE!>";
const SERVICE_SECONDS: i32 = 5;
const AI_TICK: u64 = 4;
const NO_PRIORITY: i32 = -99_999;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Factory {
    pub(super) entity: Entity,
    pub(super) idle: bool,
}

impl RotwkRules {
    pub(in crate::rules) fn seed_team_builders(&self, state: &mut MapScriptState) {
        #[allow(clippy::cast_possible_truncation)]
        let seconds = self.ai_timing.team_seconds as i32;
        state.rt.builders = super::team_build::Builders::seeded(
            self.computer_sides.iter().map(|side| side.player),
            seconds,
        );
    }

    pub(in crate::rules) fn step_team_builders(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        anchor: Option<Entity>,
    ) -> Result<Vec<(Entity, f32)>> {
        let (Some(engine), Some(anchor)) = (self.map_scripts.as_ref(), anchor) else {
            return Ok(Vec::new());
        };
        if tick.frame().get() % TICKS_PER_SCRIPT_FRAME != AI_TICK {
            return Ok(Vec::new());
        }
        let Some(state) = tick.state().get(ids.map_script_state, anchor) else {
            return Ok(Vec::new());
        };
        if state.rt.builders.is_empty() {
            return Ok(Vec::new());
        }
        let mut state = state.clone();
        let mut kills = Vec::new();
        for player in state.rt.builders.players() {
            self.update_builder(tick, ids, engine, &mut state, player, &mut kills)?;
        }
        tick.state_mut()
            .insert(ids.map_script_state, anchor, state)?;
        Ok(kills)
    }

    fn update_builder(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: PlayerId,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        self.check_ready_teams(tick, ids, engine, state, player);
        self.check_queued_teams(tick, ids, engine, state, player, kills)?;
        self.do_team_building(tick, ids, engine, state, player, kills)
    }

    fn do_team_building(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: PlayerId,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        if !self.can_build_units(tick.state(), ids, player) {
            return Ok(());
        }
        let ready = {
            let Some(builder) = state.rt.builders.get_mut(player) else {
                return Ok(());
            };
            if !builder.ready {
                builder.team_timer = builder.team_timer.saturating_sub(1);
                if builder.team_timer <= 0 {
                    builder.ready = true;
                    builder.team_delay = 0;
                }
            }
            builder.team_delay = builder.team_delay.saturating_sub(1);
            if builder.team_delay >= 1 {
                return Ok(());
            }
            builder.ready
        };
        self.queue_units(tick, ids, engine, state, player)?;
        if ready && self.select_team_to_build(tick, ids, engine, state, player, kills)? {
            self.queue_units(tick, ids, engine, state, player)?;
        }
        if let Some(builder) = state.rt.builders.get_mut(player) {
            builder.team_delay = SERVICE_SECONDS.saturating_mul(logic_hz());
        }
        Ok(())
    }

    fn can_build_units(&self, world: &World, ids: Ids, player: PlayerId) -> bool {
        crate::economy::player_entity(world, Self::economy_ids(ids), player)
            .and_then(|entity| world.get(ids.build_switches, entity))
            .is_none_or(|switches| switches.can_build_units)
    }

    pub(super) fn proto_controller(
        &self,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        proto: usize,
    ) -> Option<PlayerId> {
        state
            .rt
            .teams
            .controller(proto)
            .or_else(|| self.resolve_side_player(&engine.team_protos.get(proto)?.owner))
    }

    fn select_team_to_build(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: PlayerId,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<bool> {
        let mut candidates: Vec<(usize, i32)> = Vec::new();
        let mut highest = NO_PRIORITY;
        for (index, proto) in engine.team_protos.iter() {
            if self.proto_controller(engine, state, index) != Some(player) {
                continue;
            }
            if self.is_a_good_idea_to_build(tick, ids, engine, state, player, index) {
                let priority = proto.template.production_priority;
                highest = highest.max(priority);
                candidates.push((index, priority));
            }
        }
        let pool: Vec<usize> = candidates
            .into_iter()
            .filter(|&(_, priority)| priority == highest)
            .map(|(index, _)| index)
            .collect();
        let count = u32::try_from(pool.len()).unwrap_or(u32::MAX);
        if count == 0 {
            return Ok(false);
        }
        let pick = usize::try_from(tick.rng().below(count)).unwrap_or(0);
        let Some(&proto) = pool.get(pick) else {
            return Ok(false);
        };
        diag!(
            "script: team-build player {} picks {:?} (priority {highest}, {count} candidate(s))",
            player.get(),
            engine.team_protos.get(proto).map(|p| p.name.as_str())
        );
        self.build_specific_ai_team(tick, ids, engine, state, player, proto, false, kills)?;
        let money = self.purse_amount(tick.state(), ids, player);
        let timing = self.ai_timing;
        if let Some(builder) = state.rt.builders.get_mut(player) {
            builder.ready = false;
            let mut timer = builder.team_seconds.saturating_mul(logic_hz());
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            if money < timing.poor_threshold {
                timer = (timer as f32 / timing.teams_poor_rate) as i32;
            } else if money > timing.wealthy_threshold {
                timer = (timer as f32 / timing.teams_wealthy_rate) as i32;
            }
            builder.team_timer = timer;
        }
        Ok(true)
    }

    fn purse_amount(&self, world: &World, ids: Ids, player: PlayerId) -> i64 {
        crate::economy::purse_of(world, Self::economy_ids(ids), player)
            .amount()
            .max(0)
    }

    fn is_a_good_idea_to_build(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: PlayerId,
        proto: usize,
    ) -> bool {
        if !self.production_condition(tick, ids, engine, state, proto, player) {
            return false;
        }
        let Some(template) = engine.team_protos.get(proto).map(|p| &p.template) else {
            return false;
        };
        let instances = i32::try_from(state.rt.teams.instances_of(proto).len()).unwrap_or(i32::MAX);
        if instances >= template.max_instances {
            return false;
        }
        let queued = state.rt.builders.get(player).is_some_and(|builder| {
            builder.build.iter().any(|entry| {
                state
                    .rt
                    .teams
                    .get(entry.team)
                    .is_some_and(|team| team.proto == proto)
            })
        });
        if queued {
            return false;
        }
        self.can_meet_minimum(tick.state(), ids, engine, state, player, proto)
            || self
                .is_possible_to_build(tick.state(), ids, engine, player, proto, true)
                .0
    }

    pub(super) fn production_condition(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        proto: usize,
        controller: PlayerId,
    ) -> bool {
        let Some(entry) = engine.team_protos.get(proto) else {
            return false;
        };
        let runtime = state.rt.builders.proto(proto);
        if runtime.always_false {
            return false;
        }
        let name = entry.template.production_condition.as_deref().unwrap_or("");
        if name == ALWAYS_TRUE {
            return true;
        }
        let frame = tick.frame().get() / TICKS_PER_SCRIPT_FRAME;
        let index = match runtime.script {
            Some(index) => {
                if frame < runtime.next_frame {
                    return false;
                }
                index
            }
            None => {
                let found = (!name.is_empty())
                    .then(|| engine.find_script(name, &entry.owner))
                    .flatten()
                    .filter(|&index| {
                        engine
                            .scripts
                            .get(index)
                            .is_some_and(|def| tier_flag(def, self.computer_tier(controller)))
                    });
                let Some(index) = found else {
                    state.rt.builders.proto_mut(proto).always_false = true;
                    return false;
                };
                diag!(
                    "script: team-build player {} watches production condition {name:?} of team {:?}",
                    controller.get(),
                    entry.name
                );
                state.rt.builders.proto_mut(proto).script = Some(index);
                index
            }
        };
        let Some(def) = engine.scripts.get(index) else {
            return false;
        };
        if def.delay_eval_seconds > 0 {
            let wait =
                u64::from(def.delay_eval_seconds.unsigned_abs()) * u64::from(RETAIL_LOGIC_HZ);
            state.rt.builders.proto_mut(proto).next_frame = frame.saturating_add(wait);
        }
        if !def.supported {
            return false;
        }
        state.rt.condition_team = None;
        self.conditions_hold(tick, ids, state, def, Some(controller))
    }

    fn computer_tier(&self, player: PlayerId) -> Difficulty {
        let own = self
            .difficulty
            .players
            .iter()
            .find(|entry| entry.player == player)
            .and_then(|entry| entry.ai);
        match (self.difficulty.multiplayer, own) {
            (true, Some(tier)) => tier,
            _ => self.difficulty.game,
        }
    }

    pub(super) fn same_template(&self, a: Kind, b: Kind) -> bool {
        a == b
            || self
                .kinds
                .get(a)
                .is_some_and(|entry| entry.build_variants.contains(&b))
            || self
                .kinds
                .get(b)
                .is_some_and(|entry| entry.build_variants.contains(&a))
    }

    fn cost_of(&self, player: PlayerId, kind: Kind) -> i64 {
        if self.template_flag(kind, "BUILD_FOR_FREE") {
            return 0;
        }
        self.buildables
            .iter()
            .find(|buildable| buildable.kind == kind)
            .map_or(0, |buildable| {
                mint_sage_economy::cost_to_build(
                    buildable.cost,
                    &mint_sage_economy::CostModifiers {
                        brutal_reduction: self.brutal_cheats_for(player).build_cost_reduction,
                        ..mint_sage_economy::CostModifiers::default()
                    },
                )
            })
    }

    fn template_flag(&self, kind: Kind, flag: &str) -> bool {
        self.kinds.get(kind).is_some_and(|entry| {
            entry
                .record
                .kind_of
                .iter()
                .any(|name| name.eq_ignore_ascii_case(flag))
        })
    }

    pub(super) fn find_factory(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        kind: Kind,
        busy_ok: bool,
    ) -> Option<Factory> {
        let store = world.store(ids.producer)?;
        let producers: Vec<(Entity, bool)> = store
            .iter()
            .map(|(entity, producer)| (entity, producer.queue.entries().is_empty()))
            .collect();
        let mut busy = None;
        for (entity, idle) in creation_order(producers) {
            if !world.is_live(entity)
                || world
                    .get(ids.owner, entity)
                    .is_none_or(|owner| owner.0 != player)
                || world.contains(ids.construction, entity)
                || world.contains(ids.sold, entity)
                || world.contains(ids.doomed, entity)
            {
                continue;
            }
            let Some(factory_kind) = world.get(ids.kind, entity).copied() else {
                continue;
            };
            let builds_it = self.kinds.get(factory_kind).is_some_and(|entry| {
                entry
                    .builds
                    .iter()
                    .any(|built| self.same_template(*built, kind))
            });
            if !builds_it || !self.template_flag(factory_kind, "FS_FACTORY") {
                continue;
            }
            if idle {
                return Some(Factory { entity, idle });
            }
            if self.busy_with_other_auto_built(world, ids, entity, kind) {
                return Some(Factory { entity, idle });
            }
            if busy_ok {
                busy = Some(Factory { entity, idle });
            }
        }
        busy
    }

    fn busy_with_other_auto_built(
        &self,
        world: &World,
        ids: Ids,
        factory: Entity,
        kind: Kind,
    ) -> bool {
        world
            .get(ids.producer, factory)
            .and_then(|producer| producer.queue.entries().first())
            .filter(|entry| !matches!(entry.kind, mint_sage_economy::EntryKind::Upgrade))
            .and_then(|entry| u16::try_from(entry.item.index()).ok())
            .and_then(|item| self.buildable_at(item))
            .is_some_and(|made| {
                !self.same_template(made.kind, kind)
                    && self
                        .kinds
                        .get(made.kind)
                        .is_some_and(|entry| entry.record.is_auto_built)
            })
    }

    fn can_meet_minimum(
        &self,
        world: &World,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        player: PlayerId,
        proto: usize,
    ) -> bool {
        let Some(template) = engine.team_protos.get(proto).map(|p| &p.template) else {
            return false;
        };
        let mut total: i64 = 0;
        for slot in &template.units {
            let Some(kind) = self.kinds.kind(&slot.template) else {
                continue;
            };
            let have = self.recruitable_count(world, ids, engine, state, player, kind);
            if have >= slot.min {
                continue;
            }
            if self.find_factory(world, ids, player, kind, false).is_none() {
                return false;
            }
            let short = i64::from(slot.min.saturating_sub(have));
            total = total.saturating_add(short.saturating_mul(self.cost_of(player, kind)));
        }
        total <= self.purse_amount(world, ids, player)
    }

    pub(super) fn is_possible_to_build(
        &self,
        world: &World,
        ids: Ids,
        engine: &MapScriptEngine,
        player: PlayerId,
        proto: usize,
        require_idle: bool,
    ) -> (bool, bool) {
        let Some(template) = engine.team_protos.get(proto).map(|p| &p.template) else {
            return (false, false);
        };
        let mut any_idle = false;
        let mut sum: i32 = 0;
        for slot in &template.units {
            let Some(kind) = self.kinds.kind(&slot.template) else {
                continue;
            };
            let Some(factory) = self.find_factory(world, ids, player, kind, true) else {
                return (false, false);
            };
            any_idle |= factory.idle;
            let cost = self.cost_of(player, kind);
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            {
                let average = slot.min.saturating_add(slot.max) as f32 * 0.5;
                sum = (average * cost as f32 + sum as f32) as i32;
            }
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let needed = (sum as f32 * self.ai_timing.team_resources_to_start) as i32;
        if self.purse_amount(world, ids, player) < i64::from(needed) {
            return (false, true);
        }
        (any_idle || !require_idle, false)
    }

    fn recruitable_count(
        &self,
        world: &World,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        player: PlayerId,
        kind: Kind,
    ) -> i32 {
        let Some(owners) = world.store(ids.owner) else {
            return 0;
        };
        let mut count = 0i32;
        for (entity, owner) in owners.iter() {
            if owner.0 != player
                || !world.is_live(entity)
                || world.contains(ids.doomed, entity)
                || state
                    .rt
                    .statuses
                    .has(entity, ScriptStatusBit::NotRecruitable)
                || world
                    .get(ids.kind, entity)
                    .is_none_or(|have| !self.same_template(*have, kind))
            {
                continue;
            }
            let recruitable = match state.rt.teams.team_of(entity) {
                None => true,
                Some(team) => state.rt.teams.get(team).is_some_and(|instance| {
                    instance.active
                        && (self.is_default_team(engine, state, team)
                            || engine
                                .team_protos
                                .get(instance.proto)
                                .is_some_and(|p| p.template.ai_recruitable))
                }),
            };
            if recruitable {
                count = count.saturating_add(1);
            }
        }
        count
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_specific_ai_team(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: PlayerId,
        proto: usize,
        priority: bool,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        if !self.can_build_units(tick.state(), ids, player) {
            return Ok(());
        }
        let Some(entry) = engine.team_protos.get(proto) else {
            return Ok(());
        };
        if priority
            && entry.singleton
            && state.rt.teams.instances_of(proto).iter().any(|&id| {
                state
                    .rt
                    .teams
                    .get(id)
                    .is_some_and(|t| !t.members().is_empty())
            })
        {
            return Ok(());
        }
        let world = tick.state();
        if !self.can_meet_minimum(world, ids, engine, state, player, proto) {
            let (possible, short_of_money) =
                self.is_possible_to_build(world, ids, engine, player, proto, false);
            if !possible && !short_of_money {
                return Ok(());
            }
        }
        let orders = orders_for(&entry.template.units, |name| self.kinds.kind(name));
        if orders.is_empty() {
            return Ok(());
        }
        let executes = entry.template.executes_actions_on_create;
        let existing = if entry.singleton {
            state.rt.teams.instances_of(proto).first().copied()
        } else {
            None
        };
        if executes {
            self.run_production_actions(tick, ids, engine, state, proto, None, player, kills)?;
        }
        let team = existing.unwrap_or_else(|| state.rt.teams.create_instance(proto, false));
        if let Some(instance) = state.rt.teams.get_mut(team) {
            instance.wait_for_build = true;
            instance.hold_create = true;
        }
        let frame = tick.frame().get() / TICKS_PER_SCRIPT_FRAME;
        if let Some(builder) = state.rt.builders.get_mut(player) {
            builder.enqueue(QueuedTeam {
                team,
                orders,
                priority,
                started: frame,
                sent_to_start: false,
                reinforce: None,
            });
            builder.team_delay = 0;
        }
        diag!(
            "script: team-build player {} begins team {} ({:?})",
            player.get(),
            team.get(),
            entry.name
        );
        if executes {
            self.run_production_actions(
                tick,
                ids,
                engine,
                state,
                proto,
                Some(team),
                player,
                kills,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_production_actions(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        proto: usize,
        team: Option<TeamId>,
        player: PlayerId,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(entry) = engine.team_protos.get(proto) else {
            return Ok(());
        };
        let index = state.rt.builders.proto(proto).script.or_else(|| {
            let name = entry.template.production_condition.as_deref()?;
            engine.find_script(name, &entry.owner)
        });
        let Some(def) = index.and_then(|index| engine.scripts.get(index)) else {
            return Ok(());
        };
        let previous = state.rt.calling_team;
        state.rt.calling_team = team;
        let mut outcome = Ok(());
        for action in def.actions.clone() {
            outcome = action.run(self, tick, ids, engine, state, Some(player), 0, kills);
            if outcome.is_err() {
                break;
            }
        }
        state.rt.calling_team = previous;
        outcome
    }
}

pub(super) fn creation_order<T>(mut walk: Vec<(Entity, T)>) -> Vec<(Entity, T)> {
    walk.sort_by_key(|(entity, _)| entity.index());
    walk
}

fn logic_hz() -> i32 {
    i32::try_from(RETAIL_LOGIC_HZ).unwrap_or(i32::MAX)
}

fn tier_flag(def: &super::ScriptDef, tier: Difficulty) -> bool {
    match tier {
        Difficulty::Easy => def.easy,
        Difficulty::Medium => def.normal,
        Difficulty::Hard | Difficulty::Brutal => def.hard,
    }
}
