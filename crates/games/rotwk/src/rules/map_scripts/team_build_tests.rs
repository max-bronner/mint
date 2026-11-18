use std::collections::BTreeMap;
use std::sync::Arc;

use mint_movement::{Facing, Vec2};
use mint_sage_map::{
    Condition, Dictionary, HeightMapData, OrCondition, Parameter, Script, ScriptAction, ScriptNode,
    ScriptTree, TeamDef, Value,
};
use mint_sage_passability::Passability;
use mint_sim::{PlayerId, Simulation};

use super::team::TeamProtos;
use super::{MapScriptEngine, MapScriptState};
use crate::economy::ComputerSide;
use crate::rules::RotwkRules;
use crate::templates::KindTable;

const DATA: &str = "\
Object ZSoldier\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 100\n  \
  BuildTime = 1\n  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n\
Object ZBarracks\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE FS_FACTORY\n  \
  CommandSet = ZSet\n  BuildCost = 500\n  BuildTime = 1\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n\
Object ZHall\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandPointBonus = 50\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n\
Object ZGrunt\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  CommandPoints = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n\
CommandSet ZSet\n  1 = Build_ZSoldier\nEnd\n\
CommandButton Build_ZSoldier\n  Command = UNIT_BUILD\n  Object = ZSoldier\nEnd\n";

const SIDE: PlayerId = PlayerId::new(2);

fn text(value: &str) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: 0.0,
        string_value: value.to_owned(),
    }
}

fn number(value: i32) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: value,
        real_value: 0.0,
        string_value: String::new(),
    }
}

fn act(name: &str, params: Vec<Parameter>) -> ScriptAction {
    ScriptAction {
        action_type: 0,
        name: name.to_owned(),
        params,
    }
}

fn script(
    name: &str,
    subroutine: bool,
    active: bool,
    condition: Option<(&str, Vec<Parameter>)>,
    actions: Vec<ScriptAction>,
) -> Script {
    let (condition_name, params) = condition.unwrap_or(("CONDITION_TRUE", Vec::new()));
    Script {
        name: name.to_owned(),
        comment: String::new(),
        condition_comment: String::new(),
        action_comment: String::new(),
        active,
        one_shot: !subroutine,
        easy: true,
        normal: true,
        hard: true,
        subroutine,
        delay_evaluation_seconds: 0,
        or_conditions: vec![OrCondition {
            conditions: vec![Condition {
                condition_type: 0,
                enabled: true,
                name: condition_name.to_owned(),
                params,
            }],
        }],
        actions,
        actions_false: Vec::new(),
        sequence: Default::default(),
    }
}

fn flag_gate(name: &str, flag: &str) -> Script {
    script(
        name,
        true,
        true,
        Some(("FLAG", vec![text(flag), number(1)])),
        Vec::new(),
    )
}

fn production_team(name: &str, priority: i32, condition: &str) -> TeamDef {
    let pairs = vec![
        ("teamProductionCondition", Value::Str(condition.to_owned())),
        ("teamProductionPriority", Value::Int(priority)),
        ("teamMaxInstances", Value::Int(1)),
        ("teamUnitType1", Value::Str("ZSoldier".to_owned())),
        ("teamUnitMinCount1", Value::Int(1)),
        ("teamUnitMaxCount1", Value::Int(1)),
    ];
    TeamDef {
        owner: String::from("Wilds"),
        name: name.to_owned(),
        singleton: false,
        properties: Dictionary::from_pairs(
            pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        ),
    }
}

fn slotted_team(name: &str, slots: &[(i32, i32)], extra: &[(&str, Value)]) -> TeamDef {
    let mut pairs = vec![
        (
            "teamProductionCondition".to_owned(),
            Value::Str("<!TRUE!>".to_owned()),
        ),
        ("teamProductionPriority".to_owned(), Value::Int(1000)),
        ("teamMaxInstances".to_owned(), Value::Int(1)),
    ];
    for (at, (min, max)) in slots.iter().enumerate() {
        let n = at + 1;
        pairs.push((
            format!("teamUnitType{n}"),
            Value::Str("ZSoldier".to_owned()),
        ));
        pairs.push((format!("teamUnitMinCount{n}"), Value::Int(*min)));
        pairs.push((format!("teamUnitMaxCount{n}"), Value::Int(*max)));
    }
    pairs.extend(extra.iter().map(|(k, v)| ((*k).to_owned(), v.clone())));
    TeamDef {
        owner: String::from("Wilds"),
        name: name.to_owned(),
        singleton: false,
        properties: Dictionary::from_pairs(pairs),
    }
}

struct Setup {
    scripts: Vec<Script>,
    teams: Vec<TeamDef>,
    factory: bool,
    cash: i64,
    idle_soldiers: usize,
    unrecruitable: bool,
    solo: Option<crate::game_data::CommandPointsSolo>,
    human_objects: Vec<&'static str>,
}

impl Setup {
    fn new(scripts: Vec<Script>, teams: Vec<TeamDef>) -> Self {
        Self {
            scripts,
            teams,
            factory: true,
            cash: 3_000,
            idle_soldiers: 0,
            unrecruitable: false,
            solo: None,
            human_objects: Vec::new(),
        }
    }

    fn start(self) -> Simulation<RotwkRules> {
        let data = crate::game::tests::data_from(DATA);
        let names = ["ZSoldier", "ZBarracks", "ZHall", "ZGrunt"].map(String::from);
        let (kinds, missing) = KindTable::closure(&data, &names);
        assert!(missing.is_empty(), "{missing:?}");
        let kinds = Arc::new(kinds);
        let production = kinds.production(&data);
        let mut spawns = Vec::new();
        if self.factory {
            let kind = kinds.kind("ZBarracks").unwrap();
            let at = Vec2::new(200.0, 200.0);
            let mut spawn = kinds
                .spawn(&data.combat, kind, at, Facing::EAST, SIDE)
                .unwrap();
            spawn.producer = kinds.producer(&production, kind, at, Facing::EAST);
            spawns.push(spawn);
        }
        for n in 0..self.idle_soldiers {
            let kind = kinds.kind("ZSoldier").unwrap();
            let at = Vec2::new(60.0 + 10.0 * n as f32, 60.0);
            spawns.push(
                kinds
                    .spawn(&data.combat, kind, at, Facing::EAST, SIDE)
                    .unwrap(),
            );
        }
        for (n, name) in self.human_objects.iter().enumerate() {
            let kind = kinds.kind(name).unwrap();
            let at = Vec2::new(300.0 + 50.0 * n as f32, 300.0);
            spawns.push(
                kinds
                    .spawn(&data.combat, kind, at, Facing::EAST, PlayerId::new(0))
                    .unwrap(),
            );
        }
        let spawn_count = spawns.len();
        let waypoints: BTreeMap<String, (Vec2, f32)> =
            [(String::from("P1"), (Vec2::new(10.0, 10.0), 0.0))].into();
        let tree = ScriptTree {
            player_slots: vec![self.scripts.into_iter().map(ScriptNode::Script).collect()],
        };
        let engine = MapScriptEngine::build(
            &[tree],
            &[vec![String::from("Wilds")]],
            &waypoints,
            &BTreeMap::new(),
            false,
        )
        .with_team_prototypes(TeamProtos::from_defs(&self.teams).unwrap());
        let map = HeightMapData {
            width: 96,
            height: 96,
            border_size: 0,
            boundaries: Vec::new(),
            samples: vec![0; 96 * 96],
        };
        let rules = RotwkRules::new(Passability::build(&map, 5.0 / 128.0).unwrap(), spawns)
            .with_players(2)
            .with_teams(vec![None, None])
            .with_map_players(
                2,
                vec![String::from("Wilds")],
                vec![crate::alliance::MapRelations::default()],
            )
            .with_script_local_player(Some(PlayerId::new(0)))
            .with_campaign_command_points(self.solo)
            .with_computer_sides(vec![ComputerSide {
                player: SIDE,
                evil: true,
                cash: self.cash,
            }])
            .with_map_scripts(
                Some(engine),
                vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
            )
            .with_placed_keys(if self.unrecruitable {
                let keys = crate::map_keys::ObjectKeys {
                    recruitable: Some(false),
                    ..Default::default()
                };
                (usize::from(self.factory)..spawn_count)
                    .map(|index| (index, keys.clone()))
                    .collect()
            } else {
                Vec::new()
            })
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production.clone())
            .with_economy(production.buildables.clone(), 1_000)
            .with_menus(production.menus.clone())
            .with_menu_needs(production.menu_needs.clone())
            .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
        Simulation::new(5, rules).unwrap()
    }
}

fn state_of(sim: &Simulation<RotwkRules>) -> MapScriptState {
    let ids = sim.rules().ids().unwrap();
    sim.state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, state)| state.clone()))
        .unwrap()
}

fn can_build_units(sim: &Simulation<RotwkRules>) -> bool {
    let ids = sim.rules().ids().unwrap();
    let entity =
        crate::economy::player_entity(sim.state(), RotwkRules::economy_ids(ids), SIDE).unwrap();
    sim.state()
        .get(ids.build_switches, entity)
        .unwrap()
        .can_build_units
}

#[derive(Default)]
struct Watch {
    seen: Vec<(u32, String, u64)>,
}

impl Watch {
    fn run(&mut self, sim: &mut Simulation<RotwkRules>, ticks: u64) {
        for _ in 0..ticks {
            sim.tick().unwrap();
            self.look(sim);
        }
    }

    fn look(&mut self, sim: &Simulation<RotwkRules>) {
        let state = state_of(sim);
        let Some(builder) = state.rt.builders.get(SIDE) else {
            return;
        };
        let engine = sim.rules().map_scripts.as_ref().unwrap();
        for entry in builder.build.iter().chain(&builder.ready_queue) {
            if self.seen.iter().any(|(id, ..)| *id == entry.team.get()) {
                continue;
            }
            let proto = state.rt.teams.get(entry.team).unwrap().proto;
            let name = engine.team_prototypes().get(proto).unwrap().name.clone();
            self.seen.push((entry.team.get(), name, entry.started));
        }
    }

    fn starts(&self) -> Vec<(String, u64)> {
        self.seen
            .iter()
            .map(|(_, name, at)| (name.clone(), *at))
            .collect()
    }
}

fn enable() -> ScriptAction {
    act("PLAYER_ENABLE_UNIT_CONSTRUCTION", vec![text("Wilds")])
}

fn raise(flag: &str) -> ScriptAction {
    act("SET_FLAG", vec![text(flag), number(1)])
}

#[test]
fn a_computer_side_cannot_build_units_until_a_script_enables_it() {
    let start = script("Start", false, true, None, vec![enable()]);
    let mut sim = Setup::new(vec![start], Vec::new()).start();
    assert!(!can_build_units(&sim));
    for _ in 0..12 {
        sim.tick().unwrap();
    }
    assert!(can_build_units(&sim));
}

#[test]
fn a_production_condition_holds_only_once_its_flag_is_raised_and_a_missing_script_latches_false() {
    let start = script("Start", false, true, None, vec![enable()]);
    let late = script(
        "Late",
        false,
        true,
        Some(("TIMER_EXPIRED", vec![text("T")])),
        vec![raise("Go")],
    );
    let arm = script(
        "Arm",
        false,
        true,
        None,
        vec![act("SET_TIMER", vec![text("T"), number(20)])],
    );
    let mut sim = Setup::new(
        vec![start, arm, late, flag_gate("Gate", "Go")],
        vec![
            production_team("teamReal", 1000, "Gate"),
            production_team("teamGhost", 1000, "NoSuchScript"),
        ],
    )
    .start();
    let mut watch = Watch::default();
    watch.run(&mut sim, 6 * 15);
    assert!(watch.starts().is_empty(), "nothing before the flag");
    watch.run(&mut sim, 6 * 20);
    let queued = watch.starts();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].0, "teamReal");
    let state = state_of(&sim);
    let engine = sim.rules().map_scripts.as_ref().unwrap();
    let ghost = engine.team_prototypes().find("Wilds", "teamGhost").unwrap();
    assert!(state.rt.builders.proto(ghost).always_false);
}

#[test]
fn a_production_condition_ignores_the_scripts_active_flag_and_true_needs_no_script() {
    let start = script("Start", false, true, None, vec![enable(), raise("Go")]);
    let mut gate = flag_gate("Gate", "Go");
    gate.active = false;
    let mut sim = Setup::new(
        vec![start, gate],
        vec![
            production_team("teamGated", 1000, "Gate"),
            production_team("teamFree", 900, "<!TRUE!>"),
        ],
    )
    .start();
    let mut watch = Watch::default();
    watch.run(&mut sim, 6 * 60);
    let names: Vec<String> = watch.starts().into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, vec!["teamGated", "teamFree"]);
}

#[test]
fn the_highest_priority_is_picked_first_and_the_next_after_the_team_timer() {
    let start = script("Start", false, true, None, vec![enable(), raise("Go")]);
    let mut sim = Setup::new(
        vec![start, flag_gate("Gate", "Go")],
        vec![
            production_team("teamLow", 900, "Gate"),
            production_team("teamHigh", 1000, "Gate"),
        ],
    )
    .start();
    let mut watch = Watch::default();
    watch.run(&mut sim, 6 * 70);
    let queued = watch.starts();
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[0].0, "teamHigh");
    assert_eq!(queued[1].0, "teamLow");
    assert_eq!(queued[1].1 - queued[0].1, 50);
}

#[test]
fn a_poor_purse_shortens_the_wait_by_the_poor_rate() {
    let start = script("Start", false, true, None, vec![enable(), raise("Go")]);
    let mut setup = Setup::new(
        vec![start, flag_gate("Gate", "Go")],
        vec![
            production_team("teamLow", 900, "Gate"),
            production_team("teamHigh", 1000, "Gate"),
        ],
    );
    setup.cash = 1_500;
    let mut sim = setup.start();
    let mut watch = Watch::default();
    watch.run(&mut sim, 6 * 100);
    let queued = watch.starts();
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[1].1 - queued[0].1, 83);
}

#[test]
fn every_pick_draws_once_from_the_sim_rng_even_with_one_candidate() {
    let start = script("Start", false, true, None, vec![enable(), raise("Go")]);
    let mut sim = Setup::new(
        vec![start, flag_gate("Gate", "Go")],
        vec![production_team("teamOnly", 1000, "Gate")],
    )
    .start();
    let mut watch = Watch::default();
    let mut moved = 0;
    for _ in 0..(6 * 10) {
        let before = sim.rng_state();
        let queued_before = watch.starts().len();
        watch.run(&mut sim, 1);
        if watch.starts().len() > queued_before {
            assert_ne!(sim.rng_state(), before, "the pick must draw");
            moved += 1;
        }
    }
    assert_eq!(moved, 1);
}

#[test]
fn a_team_with_no_factory_to_build_it_is_never_picked() {
    let start = script("Start", false, true, None, vec![enable(), raise("Go")]);
    let mut setup = Setup::new(
        vec![start, flag_gate("Gate", "Go")],
        vec![production_team("teamOnly", 1000, "Gate")],
    );
    setup.factory = false;
    let mut sim = setup.start();
    let mut watch = Watch::default();
    watch.run(&mut sim, 6 * 30);
    assert!(watch.starts().is_empty());
}

fn the_team(sim: &Simulation<RotwkRules>) -> Option<(bool, usize)> {
    let state = state_of(sim);
    let team = state.rt.teams.instances().next()?;
    Some((team.active, team.members().len()))
}

fn factory_queue_depth(sim: &Simulation<RotwkRules>) -> usize {
    let ids = sim.rules().ids().unwrap();
    sim.state().store(ids.producer).map_or(0, |store| {
        store
            .iter()
            .map(|(_, producer)| producer.queue.entries().len())
            .max()
            .unwrap_or(0)
    })
}

fn run_started(setup: Setup, ticks: u64) -> (Simulation<RotwkRules>, usize) {
    let mut sim = setup.start();
    let mut deepest = 0;
    for _ in 0..ticks {
        sim.tick().unwrap();
        deepest = deepest.max(factory_queue_depth(&sim));
    }
    (sim, deepest)
}

fn started(slots: &[(i32, i32)]) -> Setup {
    started_with(slots, &[])
}

fn started_with(slots: &[(i32, i32)], extra: &[(&str, Value)]) -> Setup {
    let start = script("Start", false, true, None, vec![enable()]);
    let created = script("Created", true, true, None, vec![raise("Done")]);
    Setup::new(
        vec![start, created],
        vec![slotted_team("teamQ", slots, extra)],
    )
}

fn on_create() -> [(&'static str, Value); 1] {
    [("teamOnCreateScript", Value::Str("Created".to_owned()))]
}

fn done(sim: &Simulation<RotwkRules>) -> bool {
    state_of(sim)
        .flags
        .iter()
        .any(|(key, value)| key.name == "Done" && *value)
}

fn ready_len(sim: &Simulation<RotwkRules>) -> usize {
    state_of(sim)
        .rt
        .builders
        .get(SIDE)
        .map_or(0, |builder| builder.ready_queue.len())
}

#[test]
fn a_factory_takes_one_order_at_a_time_and_each_unit_joins_the_inactive_team() {
    let mut sim = started(&[(1, 1), (1, 1)]).start();
    let mut deepest = 0;
    let mut joined_while_queued = 0;
    for _ in 0..(6 * 120) {
        sim.tick().unwrap();
        deepest = deepest.max(factory_queue_depth(&sim));
        if let Some((active, members)) = the_team(&sim) {
            if !state_of(&sim)
                .rt
                .builders
                .get(SIDE)
                .unwrap()
                .build
                .is_empty()
            {
                assert!(!active, "a trained team stays inactive while queued");
                joined_while_queued = joined_while_queued.max(members);
            }
        }
    }
    assert_eq!(deepest, 1, "never two units in the factory's queue");
    assert_eq!(joined_while_queued, 2);
}

#[test]
fn an_idle_unit_is_recruited_instead_of_trained_and_wakes_the_team() {
    let mut setup = started(&[(2, 2)]);
    setup.idle_soldiers = 1;
    let mut sim = setup.start();
    while the_team(&sim).is_none_or(|(active, _)| !active) {
        assert!(sim.frame().get() < 6 * 40, "the team never woke");
        sim.tick().unwrap();
    }
    assert_eq!(the_team(&sim), Some((true, 1)));
    assert_eq!(
        factory_queue_depth(&sim),
        1,
        "only the missing unit is trained"
    );
}

#[test]
fn a_unit_placed_not_recruitable_is_trained_around_not_recruited() {
    let mut setup = started(&[(2, 2)]);
    setup.idle_soldiers = 1;
    setup.unrecruitable = true;
    let mut sim = setup.start();
    for _ in 0..(6 * 40) {
        sim.tick().unwrap();
    }
    let ids = sim.rules().ids().unwrap();
    let idle = sim.state().store(ids.kind).unwrap().entities()[1];
    assert_eq!(state_of(&sim).rt.teams.team_of(idle), None);
}

#[test]
fn an_optional_order_is_recruited_but_never_trained() {
    let (sim, deepest) = run_started(started(&[(0, 2)]), 6 * 60);
    assert_eq!(deepest, 0);
    assert_eq!(the_team(&sim), Some((false, 0)));
    let mut setup = started(&[(0, 2)]);
    setup.idle_soldiers = 1;
    let (sim, deepest) = run_started(setup, 6 * 8);
    assert_eq!(deepest, 0);
    assert_eq!(the_team(&sim).map(|(_, members)| members), Some(1));
}

#[test]
fn a_built_team_is_released_through_the_ready_queue_and_runs_its_on_create_only_then() {
    let mut sim = started_with(&[(1, 1)], &on_create()).start();
    let mut released_at = None;
    for _ in 0..(6 * 120) {
        sim.tick().unwrap();
        if ready_len(&sim) > 0 {
            assert!(!done(&sim), "OnCreate ran while the team was still waiting");
        }
        if released_at.is_none() && done(&sim) {
            released_at = Some(sim.frame().get());
        }
    }
    assert!(released_at.is_some(), "the built team never ran OnCreate");
    assert_eq!(ready_len(&sim), 0);
    assert!(state_of(&sim)
        .rt
        .builders
        .get(SIDE)
        .unwrap()
        .build
        .is_empty());
}

#[test]
fn a_team_that_cannot_reach_its_minimum_in_time_disbands_and_never_runs_its_on_create() {
    let mut extra = on_create().to_vec();
    extra.push(("teamInitialIdleSeconds", Value::Int(1)));
    let mut setup = started_with(&[(9, 9)], &extra);
    setup.idle_soldiers = 1;
    let (sim, _) = run_started(setup, 6 * 60);
    let state = state_of(&sim);
    let builder = state.rt.builders.get(SIDE).unwrap();
    assert!(builder.build.is_empty() && builder.ready_queue.is_empty());
    assert!(!done(&sim), "a disbanded team must not run OnCreate");
    assert_eq!(
        state.rt.teams.instances().count(),
        0,
        "the emptied team is swept"
    );
}

fn build_team_script(actions: Vec<ScriptAction>) -> Setup {
    let start = script("Start", false, true, None, actions);
    Setup::new(
        vec![start],
        vec![production_team("teamQ", 1000, "NoSuchScript")],
    )
}

fn build_team() -> ScriptAction {
    act("BUILD_TEAM", vec![text("teamQ")])
}

#[test]
fn build_team_queues_a_priority_entry_only_while_unit_construction_is_enabled() {
    let mut off = build_team_script(vec![build_team()]).start();
    let mut watch = Watch::default();
    watch.run(&mut off, 6 * 4);
    assert!(
        watch.starts().is_empty(),
        "a computer side that may not build builds nothing"
    );

    let mut on = build_team_script(vec![enable(), build_team()]).start();
    let mut watch = Watch::default();
    watch.run(&mut on, 6 * 4);
    assert_eq!(watch.starts().len(), 1);
    let state = state_of(&on);
    let entry = state.rt.builders.get(SIDE).unwrap().build.first().unwrap();
    assert!(entry.priority);
}

#[test]
fn set_base_construction_speed_sets_the_builders_team_seconds() {
    let seconds = act(
        "SET_BASE_CONSTRUCTION_SPEED",
        vec![text("Wilds"), number(3)],
    );
    let mut sim = build_team_script(vec![seconds]).start();
    for _ in 0..12 {
        sim.tick().unwrap();
    }
    assert_eq!(
        state_of(&sim).rt.builders.get(SIDE).unwrap().team_seconds,
        3
    );
}

fn points(sim: &Simulation<RotwkRules>, player: PlayerId) -> (i32, i32) {
    let ids = sim.rules().ids().unwrap();
    let pool = crate::economy::points_of(sim.state(), RotwkRules::economy_ids(ids), player);
    (pool.used(), pool.limit())
}

fn campaign_pairs() -> crate::game_data::CommandPointsSolo {
    crate::game_data::CommandPointsSolo {
        good: Some((120, 150)),
        evil: Some((120, 150)),
        good_ai: Some((640, 650)),
        evil_ai: Some((640, 650)),
        ..Default::default()
    }
}

#[test]
fn a_campaign_human_starts_on_the_solo_pair_and_a_computer_side_on_the_ai_pair() {
    let mut setup = Setup::new(Vec::new(), Vec::new());
    setup.solo = Some(campaign_pairs());
    let mut sim = setup.start();
    sim.tick().unwrap();
    assert_eq!(points(&sim, PlayerId::new(0)).1, 120);
    assert_eq!(points(&sim, SIDE).1, 640);
}

#[test]
fn an_override_sets_base_and_cap_and_leaves_used_to_the_recount() {
    let override_points = |who: &str, base: i32, cap: i32| {
        act(
            "OVERRIDE_PLAYER_COMMAND_POINTS",
            vec![text(who), number(base), number(cap)],
        )
    };
    let start = script(
        "Start",
        false,
        true,
        None,
        vec![
            override_points("<Local Player>", 250, 600),
            override_points("Wilds", 10_000, 10_000),
        ],
    );
    let mut setup = Setup::new(vec![start], Vec::new());
    setup.solo = Some(campaign_pairs());
    setup.human_objects = vec!["ZHall", "ZGrunt"];
    let mut sim = setup.start();
    for _ in 0..12 {
        sim.tick().unwrap();
    }
    assert_eq!(points(&sim, PlayerId::new(0)), (10, 300));
    assert_eq!(points(&sim, SIDE).1, 10_000);
}
