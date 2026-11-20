use super::*;
use crate::rules::map_scripts::{MapScriptEngine, ScriptObjectName};
use mint_sage_map::{
    Condition, OrCondition, Parameter, Script, ScriptAction, ScriptGroup, ScriptNode, ScriptTree,
};

mod hud;
mod reveal;
mod sequential;
mod team_hooks;
mod tutorial;
mod walk;

const DATA: &str = "\
Object ZGollum\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n";

fn string_param(value: &str) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: 0.0,
        string_value: value.to_owned(),
    }
}

fn int_param(value: i32) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: value,
        real_value: 0.0,
        string_value: String::new(),
    }
}

fn real_param(value: f32) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: value,
        string_value: String::new(),
    }
}

fn condition(name: &str, params: Vec<Parameter>) -> OrCondition {
    OrCondition {
        conditions: vec![Condition {
            condition_type: 0,
            inverted: false,
            enabled: true,
            name: name.to_owned(),
            params,
        }],
    }
}

fn action(name: &str, params: Vec<Parameter>) -> ScriptAction {
    ScriptAction {
        action_type: 0,
        enabled: true,
        name: name.to_owned(),
        params,
    }
}

fn script(
    name: &str,
    active: bool,
    one_shot: bool,
    or_conditions: Vec<OrCondition>,
    actions: Vec<ScriptAction>,
    actions_false: Vec<ScriptAction>,
) -> Script {
    Script {
        name: name.to_owned(),
        comment: String::new(),
        condition_comment: String::new(),
        action_comment: String::new(),
        active,
        one_shot,
        easy: true,
        normal: true,
        hard: true,
        subroutine: false,
        delay_evaluation_seconds: 0,
        or_conditions,
        actions,
        actions_false,
        sequence: Default::default(),
    }
}

fn gollum_spawn_tree() -> ScriptTree {
    let pick = script(
        "PickSpawnPoint",
        true,
        true,
        vec![condition(
            "IS_GAME_MODE_ACTIVE",
            vec![string_param("ringheroes")],
        )],
        vec![
            action(
                "SET_RANDOM_COUNTER",
                vec![string_param("SpawnPoint"), int_param(2), int_param(2)],
            ),
            action("ENABLE_SCRIPT", vec![string_param("Spawnpoints")]),
        ],
        vec![action(
            "DISABLE_SCRIPT",
            vec![string_param("PickSpawnPoint")],
        )],
    );
    let spawn1 = script(
        "Spawn1",
        true,
        true,
        vec![condition(
            "COUNTER",
            vec![string_param("SpawnPoint"), int_param(2), int_param(1)],
        )],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Gollum"),
                string_param("ZGollum"),
                string_param("teamWilds"),
                string_param("SpawnPoint_1"),
            ],
        )],
        Vec::new(),
    );
    let spawn2 = script(
        "Spawn2",
        true,
        true,
        vec![condition(
            "COUNTER",
            vec![string_param("SpawnPoint"), int_param(2), int_param(2)],
        )],
        vec![
            action(
                "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
                vec![
                    string_param("Gollum"),
                    string_param("ZGollum"),
                    string_param("teamWilds"),
                    string_param("SpawnPoint_2"),
                ],
            ),
            action("ENABLE_SCRIPT", vec![string_param("GollumExists")]),
        ],
        Vec::new(),
    );
    let group = ScriptGroup {
        name: "Spawnpoints".to_owned(),
        active: false,
        is_subroutine: false,
        children: vec![ScriptNode::Script(spawn1), ScriptNode::Script(spawn2)],
    };
    let exists = script(
        "GollumExists",
        false,
        true,
        vec![condition("NAMED_CREATED", vec![string_param("Gollum")])],
        vec![action("ENABLE_SCRIPT", vec![string_param("Cleanup")])],
        vec![action(
            "ENABLE_SCRIPT",
            vec![string_param("PickSpawnPoint")],
        )],
    );
    let cleanup = script(
        "Cleanup",
        false,
        false,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![action("DISABLE_SCRIPT", vec![string_param("Spawnpoints")])],
        Vec::new(),
    );
    ScriptTree {
        player_slots: vec![vec![
            ScriptNode::Script(pick),
            ScriptNode::Group(group),
            ScriptNode::Script(exists),
            ScriptNode::Script(cleanup),
        ]],
    }
}

fn run(allow_ring_heroes: bool, ticks: u32) -> Vec<(String, Vec2)> {
    run_with(DATA, allow_ring_heroes, ticks)
}

fn run_with(text: &str, allow_ring_heroes: bool, ticks: u32) -> Vec<(String, Vec2)> {
    run_tree(text, gollum_spawn_tree(), &[], allow_ring_heroes, ticks)
}

fn run_tree(
    text: &str,
    tree: ScriptTree,
    tree_sides: &[Vec<String>],
    allow_ring_heroes: bool,
    ticks: u32,
) -> Vec<(String, Vec2)> {
    let data = crate::game::tests::data_from(text);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZGollum")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: std::collections::BTreeMap<String, (Vec2, f32)> = [
        (String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0)),
        (String::from("SpawnPoint_2"), (Vec2::new(20.0, 30.0), 0.0)),
    ]
    .into_iter()
    .collect();
    let engine = MapScriptEngine::build(
        &[tree],
        tree_sides,
        &waypoints,
        &std::collections::BTreeMap::new(),
        allow_ring_heroes,
    );
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    world
        .store(ids.script_name)
        .map(|store| {
            store
                .iter()
                .filter(|(entity, _)| world.is_live(*entity))
                .filter_map(|(entity, tag): (Entity, &ScriptObjectName)| {
                    let body = world.get(ids.body, entity)?;
                    Some((tag.0.clone(), body.0.position))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn run_nested_group(middle_active: bool, ticks: u32) -> Vec<(String, Vec2)> {
    let spawn = script(
        "Spawn",
        true,
        true,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Gollum"),
                string_param("ZGollum"),
                string_param("teamWilds"),
                string_param("SpawnPoint_1"),
            ],
        )],
        Vec::new(),
    );
    let inner = ScriptGroup {
        name: "Inner".to_owned(),
        active: true,
        is_subroutine: false,
        children: vec![ScriptNode::Script(spawn)],
    };
    let middle = ScriptGroup {
        name: "Middle".to_owned(),
        active: middle_active,
        is_subroutine: false,
        children: vec![ScriptNode::Group(inner)],
    };
    let root = ScriptGroup {
        name: "Root".to_owned(),
        active: true,
        is_subroutine: false,
        children: vec![ScriptNode::Group(middle)],
    };
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Group(root)]],
    };
    let data = crate::game::tests::data_from(DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZGollum")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: std::collections::BTreeMap<String, (Vec2, f32)> =
        [(String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0))]
            .into_iter()
            .collect();
    let engine = MapScriptEngine::build(
        &[tree],
        &[],
        &waypoints,
        &std::collections::BTreeMap::new(),
        true,
    );
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    world
        .store(ids.script_name)
        .map(|store| {
            store
                .iter()
                .filter(|(entity, _)| world.is_live(*entity))
                .filter_map(|(entity, tag): (Entity, &ScriptObjectName)| {
                    let body = world.get(ids.body, entity)?;
                    Some((tag.0.clone(), body.0.position))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_inactive_middle_group_blocks_its_grandchild_group_too() {
    assert!(run_nested_group(false, 10).is_empty());
}

#[test]
fn an_active_chain_of_nested_groups_runs_the_grandchild() {
    let named = run_nested_group(true, 10);
    assert_eq!(named.len(), 1, "{named:?}");
}

#[test]
fn gollum_spawns_exactly_once_at_the_drawn_waypoint() {
    let named = run(true, 20);
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0].0, "Gollum");
    assert_eq!(named[0].1, Vec2::new(20.0, 30.0));
}

const WANDERING: &str = "Object ZGollum
  Side = Neutral
  KindOf = INFANTRY SELECTABLE
    Body = ActiveBody Tag_Body
    MaxHealth = 10
  End
    Behavior = AnimalAIUpdate Tag_AI
    WanderPercentage = 100
        MaxWanderDistance = 20
    MaxWanderRadius = 999
  End
    Geometry = CYLINDER
  GeometryMajorRadius = 4
  GeometryHeight = 10
End
";

#[test]
fn a_script_created_animal_wanders_off_its_waypoint() {
    let named = run_with(WANDERING, true, 150);
    assert_eq!(named.len(), 1, "{named:?}");
    assert_ne!(named[0].1, Vec2::new(20.0, 30.0));
}

fn gated_tree(easy: bool, normal: bool, hard: bool) -> ScriptTree {
    fn gate(nodes: &mut [ScriptNode], flags: (bool, bool, bool)) {
        for node in nodes {
            match node {
                ScriptNode::Script(s) => (s.easy, s.normal, s.hard) = flags,
                ScriptNode::Group(g) => gate(&mut g.children, flags),
            }
        }
    }
    let mut tree = gollum_spawn_tree();
    for slot in &mut tree.player_slots {
        gate(slot, (easy, normal, hard));
    }
    tree
}

#[test]
fn a_player_without_a_difficulty_runs_only_normal_scripts() {
    let sides = [vec![String::from("Player_1")]];
    let normal_only = run_tree(DATA, gated_tree(false, true, false), &sides, true, 20);
    assert_eq!(normal_only.len(), 1, "{normal_only:?}");
    let hard_only = run_tree(DATA, gated_tree(false, false, true), &sides, true, 20);
    assert!(hard_only.is_empty(), "{hard_only:?}");
}

#[test]
fn gollum_does_not_spawn_with_the_rule_off() {
    let named = run(false, 20);
    assert!(named.is_empty(), "{named:?}");
}

#[test]
fn a_one_shot_script_does_not_refire_while_its_group_stays_active() {
    let spawn = script(
        "Spawn",
        true,
        true,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Gollum"),
                string_param("ZGollum"),
                string_param("teamWilds"),
                string_param("SpawnPoint_1"),
            ],
        )],
        Vec::new(),
    );
    let group = ScriptGroup {
        name: "Spawnpoints".to_owned(),
        active: true,
        is_subroutine: false,
        children: vec![ScriptNode::Script(spawn)],
    };
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Group(group)]],
    };
    let data = crate::game::tests::data_from(DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZGollum")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: std::collections::BTreeMap<String, (Vec2, f32)> =
        [(String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0))]
            .into_iter()
            .collect();
    let engine = MapScriptEngine::build(
        &[tree],
        &[],
        &waypoints,
        &std::collections::BTreeMap::new(),
        true,
    );
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let count = world
        .store(ids.script_name)
        .map(|store| store.iter().filter(|(e, _)| world.is_live(*e)).count())
        .unwrap_or(0);
    assert_eq!(count, 1, "a one-shot script fired more than once");
}

fn run_timer(ticks: u32) -> Vec<(String, Vec2)> {
    run_timer_with(
        action(
            "SET_MILLISECOND_TIMER",
            vec![string_param("T"), real_param(1.0)],
        ),
        ticks,
    )
}

fn run_timer_with(set: ScriptAction, ticks: u32) -> Vec<(String, Vec2)> {
    let set_timer = script(
        "SetIt",
        true,
        true,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![set],
        Vec::new(),
    );
    let check = script(
        "Check",
        true,
        true,
        vec![condition("TIMER_EXPIRED", vec![string_param("T")])],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Gollum"),
                string_param("ZGollum"),
                string_param("teamWilds"),
                string_param("SpawnPoint_1"),
            ],
        )],
        Vec::new(),
    );
    let tree = ScriptTree {
        player_slots: vec![vec![
            ScriptNode::Script(set_timer),
            ScriptNode::Script(check),
        ]],
    };
    let data = crate::game::tests::data_from(DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZGollum")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: std::collections::BTreeMap<String, (Vec2, f32)> =
        [(String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0))]
            .into_iter()
            .collect();
    let engine = MapScriptEngine::build(
        &[tree],
        &[],
        &waypoints,
        &std::collections::BTreeMap::new(),
        true,
    );
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    world
        .store(ids.script_name)
        .map(|store| {
            store
                .iter()
                .filter(|(entity, _)| world.is_live(*entity))
                .filter_map(|(entity, tag): (Entity, &ScriptObjectName)| {
                    let body = world.get(ids.body, entity)?;
                    Some((tag.0.clone(), body.0.position))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn set_timer_counts_script_frames_not_sim_ticks() {
    let five = || action("SET_TIMER", vec![string_param("T"), int_param(5)]);
    assert!(run_timer_with(five(), 30).is_empty());
    assert_eq!(run_timer_with(five(), 31).len(), 1);
}

#[test]
fn a_millisecond_timer_rounds_up_with_the_retail_f32_product() {
    let point_six = || {
        action(
            "SET_MILLISECOND_TIMER",
            vec![string_param("T"), real_param(0.6)],
        )
    };
    assert!(run_timer_with(point_six(), 24).is_empty());
    assert_eq!(run_timer_with(point_six(), 25).len(), 1);
}

#[test]
fn a_random_counter_write_leaves_a_running_countdown_running() {
    let at_first_pass = |name: &str, actions: Vec<ScriptAction>| {
        ScriptNode::Script(script(
            name,
            true,
            true,
            vec![condition("CONDITION_TRUE", Vec::new())],
            actions,
            Vec::new(),
        ))
    };
    let tree = ScriptTree {
        player_slots: vec![vec![
            at_first_pass(
                "Start",
                vec![action("SET_TIMER", vec![string_param("T"), int_param(5)])],
            ),
            at_first_pass(
                "Overwrite",
                vec![action(
                    "SET_RANDOM_COUNTER",
                    vec![string_param("T"), int_param(2), int_param(2)],
                )],
            ),
            ScriptNode::Script(script(
                "Check",
                true,
                true,
                vec![condition("TIMER_EXPIRED", vec![string_param("T")])],
                vec![spawn("Late")],
                Vec::new(),
            )),
        ]],
    };
    assert!(run_tree(DATA, tree.clone(), &[], true, 12).is_empty());
    assert_eq!(run_tree(DATA, tree, &[], true, 13).len(), 1);
}

fn spawn(name: &str) -> ScriptAction {
    action(
        "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
        vec![
            string_param(name),
            string_param("ZGollum"),
            string_param("teamWilds"),
            string_param("SpawnPoint_1"),
        ],
    )
}

#[test]
fn flags_are_keyed_by_side_and_name() {
    let set = script(
        "SetFlags",
        true,
        true,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![
            action("SET_FLAG", vec![string_param("X"), int_param(1)]),
            action("SET_FLAG", vec![string_param("/N"), int_param(1)]),
        ],
        Vec::new(),
    );
    let reader = |name: &str, flag: &str| {
        ScriptNode::Script(script(
            name,
            true,
            true,
            vec![condition("FLAG", vec![string_param(flag), int_param(1)])],
            vec![spawn(name)],
            Vec::new(),
        ))
    };
    let tree = ScriptTree {
        player_slots: vec![
            vec![ScriptNode::Script(set)],
            vec![
                reader("Qualified", "PlyrA/X"),
                reader("Neutral", "/N"),
                reader("Bare", "X"),
            ],
        ],
    };
    let sides = [vec![String::from("PlyrA"), String::from("PlyrB")]];
    let mut names: Vec<String> = run_tree(DATA, tree, &sides, true, 13)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    names.sort();
    assert_eq!(names, ["Neutral", "Qualified"]);
}

#[test]
fn a_millisecond_timer_has_not_expired_one_frame_before_its_span() {
    assert!(run_timer(30).is_empty());
}

#[test]
fn a_millisecond_timer_has_expired_at_the_end_of_its_span() {
    assert_eq!(run_timer(31).len(), 1);
}

#[test]
fn two_slots_with_this_player_set_different_players_purses() {
    let set_money = |amount: i32| {
        script(
            "SetMoney",
            true,
            true,
            vec![condition("CONDITION_TRUE", Vec::new())],
            vec![action(
                "PLAYER_SET_MONEY",
                vec![string_param("<This Player>"), int_param(amount)],
            )],
            Vec::new(),
        )
    };
    let tree = ScriptTree {
        player_slots: vec![
            vec![ScriptNode::Script(set_money(777))],
            vec![ScriptNode::Script(set_money(555))],
        ],
    };
    let tree_sides = vec![vec![String::from("Player_1"), String::from("Player_2")]];
    let engine = MapScriptEngine::build(
        &[tree],
        &tree_sides,
        &BTreeMap::new(),
        &BTreeMap::new(),
        true,
    );
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_economy(Vec::new(), 1_000)
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        );
    let mut simulation = Simulation::new(7, rules).unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let economy = RotwkRules::economy_ids(ids);
    let purse =
        |player: PlayerId| crate::economy::purse_of(simulation.state(), economy, player).amount();
    assert_eq!(purse(PlayerId::new(0)), 777, "slot 0's own player");
    assert_eq!(purse(PlayerId::new(1)), 555, "slot 1's own player");
}

#[test]
fn delay_evaluation_ten_fires_at_most_once_per_fifty_script_frames() {
    let spawn = script(
        "Spawn",
        true,
        false,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Gollum"),
                string_param("ZGollum"),
                string_param("teamWilds"),
                string_param("SpawnPoint_1"),
            ],
        )],
        Vec::new(),
    );
    let mut with_delay = spawn;
    with_delay.delay_evaluation_seconds = 10;
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Script(with_delay)]],
    };
    let data = crate::game::tests::data_from(DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZGollum")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: BTreeMap<String, (Vec2, f32)> =
        [(String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0))]
            .into_iter()
            .collect();
    let engine = MapScriptEngine::build(&[tree], &[], &waypoints, &BTreeMap::new(), true);
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(9, rules).unwrap();
    for _ in 0..400 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let count = world
        .store(ids.script_name)
        .map(|store| store.iter().filter(|(e, _)| world.is_live(*e)).count())
        .unwrap_or(0);
    assert_eq!(count, 2, "exactly the first evaluation plus one more");
}

#[test]
fn team_player_n_resolves_through_the_start_contestant_pairs_not_the_start_index() {
    let spawn = script(
        "Spawn",
        true,
        true,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Gollum"),
                string_param("ZGollum"),
                string_param("teamPlayer_2"),
                string_param("SpawnPoint_1"),
            ],
        )],
        Vec::new(),
    );
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Script(spawn)]],
    };
    let data = crate::game::tests::data_from(DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZGollum")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: BTreeMap<String, (Vec2, f32)> =
        [(String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0))]
            .into_iter()
            .collect();
    let engine = MapScriptEngine::build(&[tree], &[], &waypoints, &BTreeMap::new(), true);
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(1)), (1, PlayerId::new(0))],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(11, rules).unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let owner = world
        .store(ids.script_name)
        .and_then(|store| store.iter().find(|(e, _)| world.is_live(*e)))
        .and_then(|(entity, _)| world.get(ids.owner, entity))
        .map(|owner| owner.0);
    assert_eq!(owner, Some(PlayerId::new(0)), "start 1 is held by player 0");
}

#[test]
fn a_start_nobody_holds_is_refused_by_name_with_the_seated_ones_listed() {
    let map = flat_map(8, 8);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_map_scripts(None, vec![(0, PlayerId::new(0)), (2, PlayerId::new(1))]);
    assert_eq!(rules.seated_side("Player_1"), Ok(PlayerId::new(0)));
    assert_eq!(rules.seated_side("player_3"), Ok(PlayerId::new(1)));
    let refused = rules.seated_side("Player_2").unwrap_err();
    assert!(
        refused.contains("Player_1, Player_3"),
        "the refusal lists the seated starts: {refused}"
    );
}

#[test]
fn named_kill_then_named_destroyed_fires_and_a_never_created_name_never_does() {
    let spawn = script(
        "Spawn",
        true,
        true,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Gollum"),
                string_param("ZGollum"),
                string_param("teamWilds"),
                string_param("SpawnPoint_1"),
            ],
        )],
        Vec::new(),
    );
    let kill_it = script(
        "KillIt",
        true,
        true,
        vec![condition("NAMED_CREATED", vec![string_param("Gollum")])],
        vec![action("NAMED_KILL", vec![string_param("Gollum")])],
        Vec::new(),
    );
    let confirm = script(
        "Confirm",
        true,
        true,
        vec![condition("NAMED_DESTROYED", vec![string_param("Gollum")])],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Confirmed"),
                string_param("ZGollum"),
                string_param("teamWilds"),
                string_param("SpawnPoint_2"),
            ],
        )],
        Vec::new(),
    );
    let wrongly_confirm = script(
        "WronglyConfirm",
        true,
        true,
        vec![condition(
            "NAMED_DESTROYED",
            vec![string_param("NeverThere")],
        )],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("WronglyConfirmed"),
                string_param("ZGollum"),
                string_param("teamWilds"),
                string_param("SpawnPoint_1"),
            ],
        )],
        Vec::new(),
    );
    let tree = ScriptTree {
        player_slots: vec![vec![
            ScriptNode::Script(spawn),
            ScriptNode::Script(kill_it),
            ScriptNode::Script(confirm),
            ScriptNode::Script(wrongly_confirm),
        ]],
    };
    let data = crate::game::tests::data_from(DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZGollum")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: BTreeMap<String, (Vec2, f32)> = [
        (String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0)),
        (String::from("SpawnPoint_2"), (Vec2::new(20.0, 30.0), 0.0)),
    ]
    .into_iter()
    .collect();
    let engine = MapScriptEngine::build(&[tree], &[], &waypoints, &BTreeMap::new(), true);
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..25 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let names: Vec<String> = world
        .store(ids.script_name)
        .map(|store| {
            store
                .iter()
                .filter(|(entity, _)| world.is_live(*entity))
                .map(|(_, tag)| tag.0.clone())
                .collect()
        })
        .unwrap_or_default();
    assert!(names.contains(&String::from("Confirmed")), "{names:?}");
    assert!(
        !names.contains(&String::from("WronglyConfirmed")),
        "{names:?}"
    );
}

fn run_reveal(ticks: u32) -> (u32, u32, u32) {
    let reveal = script(
        "Reveal",
        true,
        true,
        vec![condition("CONDITION_TRUE", vec![])],
        vec![
            action(
                "MAP_REVEAL_PERMANENTLY_IN_TRIGGER",
                vec![
                    string_param("North"),
                    string_param("Player_1"),
                    string_param("R1"),
                ],
            ),
            action(
                "MAP_REVEAL_PERMANENTLY_AT_WAYPOINT",
                vec![
                    string_param("SpawnPoint_1"),
                    real_param(5.0),
                    string_param("Player_1"),
                    string_param("R2"),
                ],
            ),
        ],
        vec![],
    );
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Script(reveal)]],
    };
    let waypoints: std::collections::BTreeMap<String, (Vec2, f32)> =
        [(String::from("SpawnPoint_1"), (Vec2::new(60.0, 60.0), 0.0))]
            .into_iter()
            .collect();
    let areas: std::collections::BTreeMap<String, Vec<(f32, f32)>> = [(
        String::from("North"),
        vec![(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0)],
    )]
    .into_iter()
    .collect();
    let engine = MapScriptEngine::build(&[tree], &[], &waypoints, &areas, true);
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        );
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let shroud_ids = RotwkRules::shroud_ids(ids);
    let resolved = PlayerId::new(0);
    let other = PlayerId::new(1);
    let (trigger_resolved, trigger_other) = crate::shroud::with_grid(world, shroud_ids, |grid| {
        let (x, y) = grid.world_to_cell(20.0, 20.0);
        (
            grid.looker_count(resolved, x, y),
            grid.looker_count(other, x, y),
        )
    })
    .unwrap_or((0, 0));
    let waypoint_resolved = crate::shroud::with_grid(world, shroud_ids, |grid| {
        let (x, y) = grid.world_to_cell(60.0, 60.0);
        grid.looker_count(resolved, x, y)
    })
    .unwrap_or(0);
    (trigger_resolved, waypoint_resolved, trigger_other)
}

#[test]
fn reveal_verbs_mark_only_the_resolved_players_grid_visible() {
    let (trigger_resolved, waypoint_resolved, trigger_other) = run_reveal(10);
    assert!(
        trigger_resolved > 0,
        "the trigger area was not revealed for the resolved player"
    );
    assert!(
        waypoint_resolved > 0,
        "the waypoint area was not revealed for the resolved player"
    );
    assert_eq!(
        trigger_other, 0,
        "an unrelated player's grid was revealed too"
    );
}

fn run_view(
    actions: Vec<ScriptAction>,
    cameras: &BTreeMap<String, mint_sage_map::NamedCamera>,
    ticks: u32,
) -> crate::rules::map_scripts::ScriptView {
    let one_shot = script(
        "View",
        true,
        true,
        vec![condition("CONDITION_TRUE", vec![])],
        actions,
        vec![],
    );
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Script(one_shot)]],
    };
    let waypoints: BTreeMap<String, (Vec2, f32)> =
        [(String::from("ShotTest"), (Vec2::new(10.0, 10.0), 0.0))]
            .into_iter()
            .collect();
    let engine = MapScriptEngine::build_with_cameras(
        &[tree],
        &[],
        &waypoints,
        &BTreeMap::new(),
        true,
        cameras,
    );
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        );
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    simulation.rules().script_view(simulation.state())
}

fn named_camera(name: &str) -> mint_sage_map::NamedCamera {
    mint_sage_map::NamedCamera {
        name: name.to_owned(),
        look_at: [50.0, 60.0, 0.0],
        pitch: 1.0,
        roll: 0.0,
        yaw: 0.5,
        zoom: 1.0,
        fov: 0.8,
        ground: 0.0,
    }
}

fn move_camera_to(name: &str) -> ScriptAction {
    action(
        "MOVE_CAMERA_TO",
        vec![
            string_param(name),
            real_param(2.5),
            real_param(0.0),
            real_param(0.5),
            real_param(0.5),
        ],
    )
}

#[test]
fn move_camera_to_prefers_a_named_camera_over_a_waypoint_and_ignores_unknown_names() {
    let cameras: BTreeMap<_, _> = [(String::from("ShotTest"), named_camera("ShotTest"))]
        .into_iter()
        .collect();
    let view = run_view(vec![move_camera_to("ShotTest")], &cameras, 2);
    let command = view.camera_move.expect("a move was issued");
    assert_eq!((command.target.x, command.target.y), (50.0, 60.0));
    assert!(command.camera.is_some());
    assert_eq!(
        (command.ms, command.ease_in_ms, command.ease_out_ms),
        (2500, 500, 500)
    );

    let waypoint_only = run_view(vec![move_camera_to("ShotTest")], &BTreeMap::new(), 2);
    let command = waypoint_only.camera_move.expect("the waypoint is a target");
    assert_eq!((command.target.x, command.target.y), (10.0, 10.0));
    assert!(command.camera.is_none());

    let unknown = run_view(vec![move_camera_to("NoSuchShot")], &cameras, 2);
    assert!(unknown.camera_move.is_none());
}

#[test]
fn camera_fade_add_follows_the_zero_hour_update_formula() {
    let fade = |up: i32| {
        action(
            "CAMERA_FADE_ADD",
            vec![
                real_param(0.0),
                real_param(1.0),
                int_param(up),
                int_param(2),
                int_param(4),
            ],
        )
    };
    let value_after = |ticks: u32| {
        run_view(vec![fade(4)], &BTreeMap::new(), ticks)
            .fade
            .map(|fade| fade.value)
    };
    let per = mint_sim::SIMULATION_HZ / 5;
    let after_frames = |frames: u32| value_after(1 + frames * per);
    assert_eq!(after_frames(0), Some(0.0));
    assert_eq!(value_after(per), Some(0.0), "no step between script frames");
    assert_eq!(after_frames(1), Some(0.25));
    assert_eq!(after_frames(4), Some(1.0));
    assert_eq!(after_frames(6), Some(1.0));
    let down = after_frames(7).expect("still fading down");
    assert!((down - 0.8).abs() < 1e-6, "{down}");
    assert_eq!(after_frames(11), None);
    assert_eq!(
        run_view(vec![fade(0)], &BTreeMap::new(), 1)
            .fade
            .map(|fade| fade.value),
        Some(1.0)
    );
}

#[test]
fn a_script_fade_is_drawn_blended_across_the_ticks_between_script_frames() {
    use crate::rules::map_scripts::script_frame_progress;
    let fade = |up: i32| {
        action(
            "CAMERA_FADE_ADD",
            vec![
                real_param(0.0),
                real_param(1.0),
                int_param(up),
                int_param(2),
                int_param(4),
            ],
        )
    };
    let drawn = |up: i32, ticks: u32, blend: f32| {
        run_view(vec![fade(up)], &BTreeMap::new(), ticks)
            .fade
            .map(|fade| fade.drawn(script_frame_progress(u64::from(ticks), blend)))
            .expect("the fade is running")
    };
    let per = mint_sim::SIMULATION_HZ / 5;
    let levels: Vec<f32> = (1 + per..1 + 2 * per)
        .map(|ticks| drawn(4, ticks, 1.0))
        .collect();
    assert!(
        levels.windows(2).all(|pair| pair[0] < pair[1]),
        "{levels:?}"
    );
    assert!(levels[0] > 0.0, "{levels:?}");
    assert_eq!(levels.last(), Some(&0.25));
    let next = drawn(4, 1 + 2 * per, 0.0);
    assert!(
        (next - 0.25).abs() < 1e-6,
        "no jump into the next script frame: {next}"
    );
    assert_eq!(drawn(0, 1, 0.0), 1.0);
}

#[test]
fn each_camera_fade_action_carries_its_own_blend() {
    use crate::rules::map_scripts::FadeKind;
    for (name, kind) in [
        ("CAMERA_FADE_ADD", FadeKind::Add),
        ("CAMERA_FADE_SUBTRACT", FadeKind::Subtract),
        ("CAMERA_FADE_SATURATE", FadeKind::Saturate),
        ("CAMERA_FADE_MULTIPLY", FadeKind::Multiply),
    ] {
        let params = vec![
            real_param(0.5),
            real_param(1.0),
            int_param(0),
            int_param(3),
            int_param(0),
        ];
        let fade = run_view(vec![action(name, params)], &BTreeMap::new(), 1).fade;
        assert_eq!(fade.map(|fade| fade.kind), Some(kind), "{name}");
    }
}

#[test]
fn view_flags_default_off_and_clip_depth_stays_unset_until_written() {
    let none = run_view(vec![], &BTreeMap::new(), 2);
    assert_eq!(none.clip_depth_multiplier, None);
    assert!(!none.ui_hidden && !none.terrain_render_disabled);
    let set = run_view(
        vec![
            action("HIDE_UI", vec![]),
            action("TERRAIN_RENDER_DISABLE", vec![int_param(1)]),
            action("SET_CAMERA_CLIP_DEPTH_MULTIPLIER", vec![real_param(2.0)]),
        ],
        &BTreeMap::new(),
        2,
    );
    assert!(set.ui_hidden && set.terrain_render_disabled);
    assert_eq!(set.clip_depth_multiplier, Some(2.0));
}

const WALKER_DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 200\nEnd\n\
Object ZGollum\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n";

fn team_def(owner: &str, name: &str, units: i32, min: i32) -> mint_sage_map::TeamDef {
    use mint_sage_map::Value;
    let text = |key: &str, value: &str| (key.to_owned(), Value::Str(value.to_owned()));
    let int = |key: &str, value: i32| (key.to_owned(), Value::Int(value));
    mint_sage_map::TeamDef {
        owner: owner.to_owned(),
        name: name.to_owned(),
        singleton: true,
        properties: mint_sage_map::Dictionary::from_pairs(vec![
            text("teamUnitType1", "ZGollum"),
            int("teamUnitMaxCount1", units),
            int("teamUnitMinCount1", min),
        ]),
    }
}

fn named_at(name: &str, waypoint: &str) -> ScriptAction {
    action(
        "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
        vec![
            string_param(name),
            string_param("ZGollum"),
            string_param("teamWilds"),
            string_param(waypoint),
        ],
    )
}

fn one_shot(name: &str, when: OrCondition, then: Vec<ScriptAction>) -> ScriptNode {
    ScriptNode::Script(script(name, true, true, vec![when], then, Vec::new()))
}

fn squad_sim(nodes: Vec<ScriptNode>, units: i32) -> Simulation<RotwkRules> {
    squad_sim_with(
        nodes,
        team_def("Wilds", "Squad", units, 1),
        WALKER_DATA,
        &["ZGollum"],
    )
}

fn squad_sim_with(
    nodes: Vec<ScriptNode>,
    team: mint_sage_map::TeamDef,
    data: &str,
    templates: &[&str],
) -> Simulation<RotwkRules> {
    squad_sim_spawning(nodes, team, data, templates, &[])
}

fn squad_sim_spawning(
    nodes: Vec<ScriptNode>,
    team: mint_sage_map::TeamDef,
    data: &str,
    templates: &[&str],
    spawns: &[(&str, f32, f32, PlayerId)],
) -> Simulation<RotwkRules> {
    let data = crate::game::tests::data_from(data);
    let roots: Vec<String> = templates.iter().map(|name| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let spawns: Vec<_> = spawns
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let waypoints: BTreeMap<String, (Vec2, f32)> = [
        (String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0)),
        (String::from("SpawnPoint_2"), (Vec2::new(60.0, 10.0), 0.0)),
    ]
    .into_iter()
    .collect();
    let tree = ScriptTree {
        player_slots: vec![nodes],
    };
    let protos = crate::rules::map_scripts::TeamProtos::from_defs(&[team]).unwrap();
    let areas: BTreeMap<String, Vec<(f32, f32)>> = [(
        String::from("Zone"),
        vec![(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)],
    )]
    .into_iter()
    .collect();
    let engine = MapScriptEngine::build(
        &[tree],
        &[vec![String::from("Wilds")]],
        &waypoints,
        &areas,
        true,
    )
    .with_team_prototypes(protos);
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    Simulation::new(5, rules).unwrap()
}

fn run_squad(nodes: Vec<ScriptNode>, units: i32, ticks: u32) -> (Vec<String>, usize, Vec2) {
    let mut simulation = squad_sim(nodes, units);
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let names = world
        .store(ids.script_name)
        .map(|store| {
            store
                .iter()
                .filter(|(entity, _)| world.is_live(*entity))
                .map(|(_, tag): (Entity, &ScriptObjectName)| tag.0.clone())
                .collect()
        })
        .unwrap_or_default();
    let owned = world
        .store(ids.owner)
        .map(|store| {
            store
                .iter()
                .filter(|(entity, owner)| world.is_live(*entity) && owner.0 == PlayerId::new(2))
                .count()
        })
        .unwrap_or(0);
    let walker = world
        .store(ids.script_name)
        .and_then(|store| {
            store
                .iter()
                .find(|(_, tag): &(Entity, &ScriptObjectName)| tag.0 == "Walker")
                .and_then(|(entity, _)| world.get(ids.body, entity))
                .map(|body| body.0.position)
        })
        .unwrap_or(Vec2::ZERO);
    (names, owned, walker)
}

#[test]
fn a_reinforcement_team_arrives_with_its_max_per_slot() {
    let reinforce = one_shot(
        "Reinforce",
        condition("CONDITION_TRUE", Vec::new()),
        vec![action(
            "CREATE_REINFORCEMENT_TEAM",
            vec![string_param("Squad"), string_param("SpawnPoint_1")],
        )],
    );
    let (_, owned, _) = run_squad(vec![reinforce], 3, 20);
    assert_eq!(owned, 3);
}

#[test]
fn a_killed_team_reads_destroyed_once_it_had_members() {
    let reinforce = one_shot(
        "Reinforce",
        condition("CONDITION_TRUE", Vec::new()),
        vec![action(
            "CREATE_REINFORCEMENT_TEAM",
            vec![string_param("Squad"), string_param("SpawnPoint_1")],
        )],
    );
    let slaughter = one_shot(
        "Slaughter",
        condition("TEAM_HAS_UNITS", vec![string_param("Squad")]),
        vec![action("TEAM_KILL", vec![string_param("Squad")])],
    );
    let report = one_shot(
        "Report",
        condition("TEAM_DESTROYED", vec![string_param("Squad")]),
        vec![named_at("Wiped", "SpawnPoint_2")],
    );
    let (names, _, _) = run_squad(vec![reinforce, slaughter, report], 2, 80);
    assert!(names.contains(&String::from("Wiped")), "{names:?}");
}

#[test]
fn a_debug_kill_of_a_team_resolves_its_deaths_and_the_team_reads_destroyed() {
    let reinforce = one_shot(
        "Reinforce",
        condition("CONDITION_TRUE", Vec::new()),
        vec![action(
            "CREATE_REINFORCEMENT_TEAM",
            vec![string_param("Squad"), string_param("SpawnPoint_1")],
        )],
    );
    let report = one_shot(
        "Report",
        condition("TEAM_DESTROYED", vec![string_param("Squad")]),
        vec![named_at("Wiped", "SpawnPoint_2")],
    );
    let mut simulation = squad_sim(vec![reinforce, report], 2);
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    simulation
        .submit(
            PlayerId::new(0),
            crate::rules::RotwkCommand::Debug(crate::rules::DebugCommand::Kill {
                target: crate::rules::DebugTarget::Team(String::from("Wilds/Squad")),
            }),
        )
        .unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let wiped = world.store(ids.script_name).is_some_and(|store| {
        store
            .iter()
            .any(|(entity, tag): (Entity, &ScriptObjectName)| {
                world.is_live(entity) && tag.0 == "Wiped"
            })
    });
    assert!(wiped, "TEAM_DESTROYED never fired");
}

#[test]
fn a_debug_kill_of_a_map_player_takes_everything_it_owns() {
    let reinforce = one_shot(
        "Reinforce",
        condition("CONDITION_TRUE", Vec::new()),
        vec![action(
            "CREATE_REINFORCEMENT_TEAM",
            vec![string_param("Squad"), string_param("SpawnPoint_1")],
        )],
    );
    let mut simulation = squad_sim(vec![reinforce], 2);
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    simulation
        .submit(
            PlayerId::new(0),
            crate::rules::RotwkCommand::Debug(crate::rules::DebugCommand::Kill {
                target: crate::rules::DebugTarget::Player(String::from("Wilds")),
            }),
        )
        .unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let owned = world.store(ids.owner).map_or(0, |store| {
        store
            .iter()
            .filter(|(entity, owner)| world.is_live(*entity) && owner.0 == PlayerId::new(2))
            .count()
    });
    assert_eq!(owned, 0);
}

#[test]
fn a_move_order_walks_the_named_unit_to_the_waypoint() {
    let make = one_shot(
        "Make",
        condition("CONDITION_TRUE", Vec::new()),
        vec![named_at("Walker", "SpawnPoint_1")],
    );
    let go = one_shot(
        "Go",
        condition("NAMED_CREATED", vec![string_param("Walker")]),
        vec![action(
            "MOVE_NAMED_UNIT_TO",
            vec![string_param("Walker"), string_param("SpawnPoint_2")],
        )],
    );
    let (_, _, at) = run_squad(vec![make, go], 1, 200);
    assert!(at.x > 40.0, "{at:?}");
}

#[test]
fn a_named_unit_is_inside_the_area_it_stands_in_and_not_outside_it() {
    let make = one_shot(
        "Make",
        condition("CONDITION_TRUE", Vec::new()),
        vec![named_at("Walker", "SpawnPoint_1")],
    );
    let inside = one_shot(
        "Inside",
        condition(
            "NAMED_INSIDE_AREA",
            vec![string_param("Walker"), string_param("Zone")],
        ),
        vec![named_at("WasInside", "SpawnPoint_2")],
    );
    let outside = one_shot(
        "Outside",
        condition(
            "NAMED_OUTSIDE_AREA",
            vec![string_param("Walker"), string_param("Zone")],
        ),
        vec![named_at("WasOutside", "SpawnPoint_2")],
    );
    let (names, _, _) = run_squad(vec![make, inside, outside], 1, 30);
    assert!(names.contains(&String::from("WasInside")), "{names:?}");
    assert!(!names.contains(&String::from("WasOutside")), "{names:?}");
}

#[test]
fn a_side_a_contestant_plays_by_name_resolves_to_that_contestant() {
    let spawn = script(
        "Spawn",
        true,
        true,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Gollum"),
                string_param("ZGollum"),
                string_param("teamPlyrHuman"),
                string_param("SpawnPoint_1"),
            ],
        )],
        Vec::new(),
    );
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Script(spawn)]],
    };
    let data = crate::game::tests::data_from(DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZGollum")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: BTreeMap<String, (Vec2, f32)> =
        [(String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0))]
            .into_iter()
            .collect();
    let engine = MapScriptEngine::build(&[tree], &[], &waypoints, &BTreeMap::new(), true);
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_scripts(Some(engine), Vec::new())
        .with_script_named_sides(vec![(String::from("PlyrHuman"), PlayerId::new(1))])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(11, rules).unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let owner = world
        .store(ids.script_name)
        .and_then(|store| store.iter().find(|(e, _)| world.is_live(*e)))
        .and_then(|(entity, _)| world.get(ids.owner, entity))
        .map(|owner| owner.0);
    assert_eq!(owner, Some(PlayerId::new(1)));
}

#[test]
fn an_object_no_script_placed_is_on_its_owners_default_team() {
    let report = one_shot(
        "Report",
        condition("TEAM_HAS_UNITS", vec![string_param("teamWilds")]),
        vec![named_at("Seen", "SpawnPoint_2")],
    );
    let mut simulation = squad_sim_with(
        vec![report],
        team_def("Wilds", "teamWilds", 1, 1),
        WALKER_DATA,
        &["ZGollum"],
    );
    let seen = |simulation: &Simulation<RotwkRules>| {
        let ids = simulation.rules().ids().unwrap();
        let world = simulation.state();
        world.store(ids.script_name).is_some_and(|store| {
            store
                .iter()
                .any(|(_, tag): (Entity, &ScriptObjectName)| tag.0 == "Seen")
        })
    };
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    assert!(!seen(&simulation));
    let data = crate::game::tests::data_from(WALKER_DATA);
    let (kinds, _) = KindTable::closure(&data, &[String::from("ZGollum")]);
    let kind = kinds.kind("ZGollum").unwrap();
    simulation
        .submit(
            PlayerId::new(2),
            RotwkCommand::Debug(DebugCommand::Spawn {
                kind: kind.0,
                x: 40.0,
                y: 40.0,
                facing: 0.0,
                role: SpawnRole::Normal,
            }),
        )
        .unwrap();
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    assert!(seen(&simulation));
}

#[test]
fn a_move_only_member_outside_does_not_keep_its_team_from_being_inside() {
    let data = format!(
        "{WALKER_DATA}Object ZMarker\n  Side = Neutral\n  KindOf = MOVE_ONLY\n  \
         Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
         Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n"
    );
    let on_squad = |name: &str, template: &str, waypoint: &str| {
        action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param(name),
                string_param(template),
                string_param("Squad"),
                string_param(waypoint),
            ],
        )
    };
    let make = one_shot(
        "Make",
        condition("CONDITION_TRUE", Vec::new()),
        vec![
            on_squad("Inside", "ZGollum", "SpawnPoint_1"),
            on_squad("Marker", "ZMarker", "SpawnPoint_2"),
        ],
    );
    let check = one_shot(
        "Check",
        condition(
            "TEAM_INSIDE_AREA_ENTIRELY",
            vec![string_param("Squad"), string_param("Zone"), int_param(0)],
        ),
        vec![named_at("AllIn", "SpawnPoint_2")],
    );
    let mut simulation = squad_sim_with(
        vec![make, check],
        team_def("Wilds", "Squad", 1, 1),
        &data,
        &["ZGollum", "ZMarker"],
    );
    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let names: Vec<String> = world
        .store(ids.script_name)
        .map(|store| store.iter().map(|(_, tag)| tag.0.clone()).collect())
        .unwrap_or_default();
    assert!(names.contains(&String::from("Marker")), "{names:?}");
    assert!(names.contains(&String::from("AllIn")), "{names:?}");
}

#[test]
fn an_object_on_no_team_counts_on_its_owners_default_team() {
    let name = one_shot(
        "Name",
        condition("CONDITION_TRUE", vec![]),
        vec![action(
            "TEAM_SET_PLAYERS_NEAREST_UNIT_OF_TYPE_TO_REFERENCE",
            vec![
                string_param("ZGollum"),
                string_param("Wilds/teamWilds"),
                string_param("Ref"),
            ],
        )],
    );
    let probe = one_shot(
        "Probe",
        condition(
            "NAMED_INSIDE_AREA",
            vec![string_param("Ref"), string_param("Zone")],
        ),
        vec![action(
            "SET_COUNTER",
            vec![string_param("Seen"), int_param(1)],
        )],
    );
    let mut simulation = squad_sim_spawning(
        vec![name, probe],
        team_def("Wilds", "teamWilds", 0, 0),
        WALKER_DATA,
        &["ZGollum"],
        &[("ZGollum", 20.0, 20.0, PlayerId::new(2))],
    );
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let state: crate::rules::map_scripts::MapScriptState = simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, s)| s.clone()))
        .unwrap();
    assert_eq!(state.counter_value("Wilds", "Seen"), 1);
}

#[test]
fn a_campaign_mission_starts_with_the_carried_skill_points_and_bounty_not_money() {
    let mut carry = mint_sage_campaign::Carryover::default();
    carry.capture(
        &[],
        &[],
        Some(&mint_sage_campaign::PlayerCapture {
            skill_points: 70.0,
            sciences: Vec::new(),
            upgrades: Vec::new(),
            cash_bounty: 0.5,
        }),
    );
    let (purse, science, bounty) = state_after_restore(carry);
    assert_eq!(
        purse,
        mint_sage_economy::Money::new(mint_sage_economy::DEFAULT_STARTING_CASH)
    );
    assert_eq!((science.skill_points, science.rank), (70, 2));
    assert_eq!(bounty, Some(0.5));
    let (_, fresh, none) = state_after_restore(mint_sage_campaign::Carryover::default());
    assert_eq!((fresh.skill_points, fresh.rank, none), (0, 1, None));
}

#[test]
fn a_scripted_victory_ends_in_a_result_the_next_roster_is_read_from() {
    let mut carry = mint_sage_campaign::Carryover::default();
    carry.capture(
        &[],
        &[],
        Some(&mint_sage_campaign::PlayerCapture {
            skill_points: 70.0,
            sciences: Vec::new(),
            upgrades: Vec::new(),
            cash_bounty: 0.5,
        }),
    );
    let tree = ScriptTree {
        player_slots: vec![vec![one_shot(
            "Win",
            condition("CONDITION_TRUE", vec![]),
            vec![action("VICTORY", vec![])],
        )]],
    };
    let mut simulation = carryover_simulation(carry, tree);
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let result = simulation.rules().match_result(simulation.state());
    let next = result.carryover().unwrap();
    assert_eq!(
        (result.end, next.skill_points(), next.cash_bounty()),
        (Some(mint_sage_campaign::MissionEnd::Victory), 70.0, 0.5)
    );
}

fn carryover_simulation(
    carry: mint_sage_campaign::Carryover,
    tree: ScriptTree,
) -> Simulation<RotwkRules> {
    let engine = MapScriptEngine::build(
        &[tree],
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        true,
    )
    .with_carryover(crate::rules::carryover::CarryoverStart {
        carry,
        delayed: Vec::new(),
        start: None,
        rally: None,
    });
    let map = flat_map(32, 32);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_spell_books(
            std::sync::Arc::new(crate::sciences::ScienceTable::default()),
            std::sync::Arc::new(crate::sciences::Ranks::from_columns(
                vec![0, 60, 120],
                vec![0, 0, 0],
            )),
            vec![crate::sciences::SpellPlan::default(); 2],
            Vec::new(),
        )
        .with_map_scripts(Some(engine), vec![(0, PlayerId::new(0))]);
    Simulation::new(5, rules).unwrap()
}

fn state_after_restore(
    carry: mint_sage_campaign::Carryover,
) -> (
    mint_sage_economy::Money,
    crate::sciences::PlayerScience,
    Option<f32>,
) {
    let mut simulation = carryover_simulation(carry, ScriptTree::default());
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let purse = crate::economy::purse_of(
        simulation.state(),
        RotwkRules::economy_ids(ids),
        PlayerId::new(0),
    );
    let player = crate::economy::player_entity(
        simulation.state(),
        RotwkRules::economy_ids(ids),
        PlayerId::new(0),
    )
    .unwrap();
    let science = simulation
        .state()
        .get(ids.player_science, player)
        .unwrap()
        .clone();
    let bounty = simulation
        .state()
        .get(ids.cash_bounty, player)
        .map(|bounty| bounty.0);
    (purse, science, bounty)
}
