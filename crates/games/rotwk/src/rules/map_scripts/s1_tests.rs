use std::collections::BTreeMap;
use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::{
    Condition, Dictionary, HeightMapData, OrCondition, Parameter, Script, ScriptAction, ScriptNode,
    ScriptTree, TeamDef,
};
use mint_sage_passability::Passability;
use mint_sim::{Entity, PlayerId, Simulation};

use super::relations::Relation;
use super::team::TeamProtos;
use super::{MapScriptEngine, MapScriptState};
use crate::rules::RotwkRules;
use crate::templates::KindTable;

const DATA: &str = "\
Object ZSoldier\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n\
Object ZArcher\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon ZSword\n  AttackRange = 50\n  DelayBetweenShots = 150\n  \
  DamageNugget\n    Damage = 5\n    DamageType = SLASH\n  End\nEnd\n\
Object ZFighter\n  Side = Neutral\n  KindOf = CAVALRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZSword\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    MoodAttackCheckRate = 500\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n\
Object ZBuilding\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n";

const P1: PlayerId = PlayerId::new(0);
const P2: PlayerId = PlayerId::new(1);

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
        enabled: true,
        name: name.to_owned(),
        params,
    }
}

fn script(
    name: &str,
    condition: Option<(&str, Vec<Parameter>)>,
    actions: Vec<ScriptAction>,
) -> Script {
    let (condition_name, params) = condition.unwrap_or(("CONDITION_TRUE", Vec::new()));
    Script {
        name: name.to_owned(),
        comment: String::new(),
        condition_comment: String::new(),
        action_comment: String::new(),
        active: true,
        one_shot: true,
        easy: true,
        normal: true,
        hard: true,
        subroutine: false,
        delay_evaluation_seconds: 0,
        or_conditions: vec![OrCondition {
            conditions: vec![Condition {
                condition_type: 0,
                inverted: false,
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

fn spawn(unit: &str, template: &str, team: &str) -> ScriptAction {
    act(
        "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
        vec![text(unit), text(template), text(team), text("P1")],
    )
}

fn team_def(name: &str) -> TeamDef {
    TeamDef {
        owner: String::new(),
        name: name.to_owned(),
        singleton: true,
        properties: Dictionary::from_pairs(Vec::new()),
    }
}

fn wilds_team(name: &str) -> TeamDef {
    TeamDef {
        owner: String::from("Wilds"),
        ..team_def(name)
    }
}

fn simulate(
    scripts: Vec<Script>,
    teams: &[TeamDef],
    local: Option<PlayerId>,
) -> Simulation<RotwkRules> {
    simulate_with(scripts, teams, local, &[], |rules| rules)
}

fn simulate_with(
    scripts: Vec<Script>,
    teams: &[TeamDef],
    local: Option<PlayerId>,
    sides: &[Vec<String>],
    tune: impl FnOnce(RotwkRules) -> RotwkRules,
) -> Simulation<RotwkRules> {
    let data = crate::game::tests::data_from(DATA);
    let names = ["ZSoldier", "ZArcher", "ZBuilding", "ZFighter"].map(String::from);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: BTreeMap<String, (Vec2, f32)> = [
        (String::from("P1"), (Vec2::new(10.0, 10.0), 0.0)),
        (String::from("P2"), (Vec2::new(300.0, 300.0), 0.0)),
    ]
    .into();
    let tree = ScriptTree {
        player_slots: vec![scripts.into_iter().map(ScriptNode::Script).collect()],
    };
    let engine = MapScriptEngine::build(&[tree], sides, &waypoints, &BTreeMap::new(), false)
        .with_team_prototypes(TeamProtos::from_defs(teams).unwrap());
    let map = HeightMapData {
        width: 96,
        height: 96,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![0; 96 * 96],
    };
    let rules = RotwkRules::new(Passability::build(&map, 5.0 / 128.0).unwrap(), Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_economy(Vec::new(), 1_000)
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_script_local_player(local)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let rules = tune(rules);
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    simulation
}

fn state_of(simulation: &Simulation<RotwkRules>) -> MapScriptState {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, state)| state.clone()))
        .unwrap()
}

fn relation(simulation: &Simulation<RotwkRules>, from: PlayerId, to: PlayerId) -> Relation {
    let state = state_of(simulation);
    simulation.rules().script_relation(&state.rt, from, to)
}

fn purse(simulation: &Simulation<RotwkRules>, player: PlayerId) -> i64 {
    let ids = simulation.rules().ids().unwrap();
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
}

fn relate(from: &str, to: &str, relation: i32) -> ScriptAction {
    act(
        "PLAYER_RELATES_PLAYER",
        vec![text(from), text(to), number(relation)],
    )
}

#[test]
fn a_player_relation_is_written_one_way_and_only_when_both_names_are_one_player() {
    let sim = simulate(
        vec![script(
            "Relate",
            None,
            vec![
                relate("Player_1", "Player_2", 2),
                relate("Player_2", "<All Players>", 2),
            ],
        )],
        &[],
        None,
    );
    assert_eq!(relation(&sim, P1, P2), Relation::Allies);
    assert_eq!(
        relation(&sim, P2, P1),
        Relation::Enemies,
        "one direction only"
    );
}

#[test]
fn a_team_override_to_a_player_ignores_tokens() {
    let relate_to = |player: &str| {
        let sim = simulate(
            vec![script(
                "Override",
                None,
                vec![
                    spawn("U", "ZSoldier", "teamWilds"),
                    act(
                        "TEAM_SET_OVERRIDE_RELATION_TO_PLAYER",
                        vec![text("teamWilds"), text(player), number(2)],
                    ),
                ],
            )],
            &[team_def("teamWilds")],
            Some(P1),
        );
        let state = state_of(&sim);
        let team = state.rt.teams.instances().next().unwrap().id;
        let wilds = (team, PlayerId::new(2));
        state
            .rt
            .relations
            .team_to_team((wilds.0, wilds.1), (wilds.0, P1), Relation::Enemies)
    };
    assert_eq!(relate_to("Player_1"), Relation::Allies);
    assert_eq!(relate_to("<Local Player>"), Relation::Enemies);
}

#[test]
fn removing_all_overrides_clears_a_teams_relations() {
    let sim = simulate(
        vec![script(
            "Override",
            None,
            vec![
                spawn("U", "ZSoldier", "teamWilds"),
                act(
                    "TEAM_SET_OVERRIDE_RELATION_TO_PLAYER",
                    vec![text("teamWilds"), text("Player_1"), number(2)],
                ),
                act(
                    "TEAM_REMOVE_ALL_OVERRIDE_RELATIONS",
                    vec![text("teamWilds")],
                ),
            ],
        )],
        &[team_def("teamWilds")],
        None,
    );
    let state = state_of(&sim);
    let team = state.rt.teams.instances().next().unwrap().id;
    assert_eq!(
        state
            .rt
            .relations
            .team_to_team((team, PlayerId::new(2)), (team, P1), Relation::Enemies),
        Relation::Enemies
    );
}

#[test]
fn a_team_attitude_reaches_every_member_and_an_out_of_range_value_changes_nothing() {
    let sim = simulate(
        vec![script(
            "Attitude",
            None,
            vec![
                spawn("U", "ZSoldier", "teamWilds"),
                act("TEAM_SET_ATTITUDE", vec![text("teamWilds"), number(-3)]),
                act("TEAM_SET_ATTITUDE", vec![text("teamWilds"), number(9)]),
            ],
        )],
        &[team_def("teamWilds")],
        None,
    );
    let state = state_of(&sim);
    let member = state.rt.teams.instances().next().unwrap().members()[0];
    assert_eq!(state.rt.attitudes.get(member), -3);
}

fn named(sim: &Simulation<RotwkRules>, name: &str) -> Entity {
    let ids = sim.rules().ids().unwrap();
    let world = sim.state();
    world
        .store(ids.script_name)
        .and_then(|store| {
            store
                .iter()
                .find(|(entity, tag)| tag.0 == name && world.is_live(*entity))
                .map(|(entity, _)| entity)
        })
        .unwrap_or_else(|| panic!("{name} was not created"))
}

#[test]
fn a_priority_list_stops_at_the_first_unknown_template_and_keeps_earlier_ones() {
    let sim = simulate(
        vec![script(
            "Priority",
            None,
            vec![
                act(
                    "OBJECTLIST_ADDOBJECTTYPE",
                    vec![text("Group"), text("ZSoldier")],
                ),
                act(
                    "OBJECTLIST_ADDOBJECTTYPE",
                    vec![text("Group"), text("ZNoSuch")],
                ),
                act(
                    "OBJECTLIST_ADDOBJECTTYPE",
                    vec![text("Group"), text("ZArcher")],
                ),
                act(
                    "SET_ATTACK_PRIORITY_THING",
                    vec![text("Set"), text("Group"), number(7)],
                ),
            ],
        )],
        &[],
        None,
    );
    let state = state_of(&sim);
    assert_eq!(state.rt.priorities.templates_of("Set"), ["zsoldier"]);
}

#[test]
fn a_kind_of_priority_reaches_every_template_with_the_bit() {
    let sim = simulate(
        vec![script(
            "Priority",
            None,
            vec![act(
                "SET_ATTACK_PRIORITY_KIND_OF",
                vec![text("Set"), text("INFANTRY"), number(4)],
            )],
        )],
        &[],
        None,
    );
    let state = state_of(&sim);
    assert_eq!(
        state.rt.priorities.templates_of("Set"),
        ["zarcher", "zsoldier"]
    );
}

#[test]
fn applying_an_undefined_set_returns_a_unit_to_the_default_set() {
    let sim = simulate(
        vec![script(
            "Apply",
            None,
            vec![
                spawn("U", "ZSoldier", "teamWilds"),
                act("SET_DEFAULT_ATTACK_PRIORITY", vec![text("Set"), number(3)]),
                act(
                    "NAMED_APPLY_ATTACK_PRIORITY_SET",
                    vec![text("U"), text("Set")],
                ),
                act(
                    "NAMED_APPLY_ATTACK_PRIORITY_SET",
                    vec![text("U"), text("Undefined")],
                ),
                spawn("V", "ZArcher", "teamWilds"),
                act(
                    "NAMED_APPLY_ATTACK_PRIORITY_SET",
                    vec![text("V"), text("Set")],
                ),
            ],
        )],
        &[],
        None,
    );
    let state = state_of(&sim);
    let (u, v) = (named(&sim, "U"), named(&sim, "V"));
    assert_eq!(state.rt.priorities.name_of(u), Some(""));
    assert_eq!(state.rt.priorities.name_of(v), Some("set"));
}

#[test]
fn give_money_adds_for_a_positive_amount_and_withdraws_for_a_negative_one() {
    let sim = simulate(
        vec![script(
            "Money",
            None,
            vec![
                act("PLAYER_GIVE_MONEY", vec![text("Player_1"), number(500)]),
                act("PLAYER_GIVE_MONEY", vec![text("Player_2"), number(-400)]),
            ],
        )],
        &[],
        None,
    );
    assert_eq!(purse(&sim, P1), 1_500);
    assert_eq!(purse(&sim, P2), 600);
}

#[test]
fn credits_and_object_counts_compare_over_the_whole_player_mask() {
    let hit = |name: &str, params: Vec<Parameter>| {
        script(
            name,
            Some((name, params)),
            vec![act("INCREMENT_COUNTER", vec![number(1), text(name)])],
        )
    };
    let sim = simulate(
        vec![
            script(
                "Setup",
                None,
                vec![
                    spawn("A", "ZSoldier", "teamWilds"),
                    spawn("B", "ZArcher", "teamWilds"),
                    act(
                        "OBJECTLIST_ADDOBJECTTYPE",
                        vec![text("Group"), text("ZSoldier")],
                    ),
                    act(
                        "OBJECTLIST_ADDOBJECTTYPE",
                        vec![text("Group"), text("ZArcher")],
                    ),
                ],
            ),
            hit(
                "PLAYER_HAS_OBJECT_COMPARISON",
                vec![text("<All Players>"), number(2), number(2), text("Group")],
            ),
            hit(
                "PLAYER_HAS_CREDITS",
                vec![number(1_999), number(0), text("<All Players>")],
            ),
            hit("NAMED_OWNED_BY_PLAYER", vec![text("A"), text("Wilds")]),
        ],
        &[],
        None,
    );
    let state = state_of(&sim);
    assert_eq!(state.counter_value("", "PLAYER_HAS_OBJECT_COMPARISON"), 1);
    assert_eq!(state.counter_value("", "PLAYER_HAS_CREDITS"), 1);
    assert_eq!(state.counter_value("", "NAMED_OWNED_BY_PLAYER"), 1);
}

#[test]
fn disabling_unit_construction_leaves_base_construction_alone() {
    let sim = simulate(
        vec![script(
            "Rights",
            None,
            vec![act(
                "PLAYER_DISABLE_UNIT_CONSTRUCTION",
                vec![text("Player_1")],
            )],
        )],
        &[],
        None,
    );
    let ids = sim.rules().ids().unwrap();
    let entity =
        crate::economy::player_entity(sim.state(), RotwkRules::economy_ids(ids), P1).unwrap();
    let switches = sim
        .state()
        .get(ids.build_switches, entity)
        .copied()
        .unwrap();
    assert!(!switches.can_build_units);
    assert!(switches.can_build_base);
}

fn fighters(merge: bool) -> (Simulation<RotwkRules>, Entity, Entity) {
    let mut actions = vec![
        spawn("A", "ZFighter", "teamPlayer_1"),
        spawn("B", "ZFighter", "teamPlayer_2"),
        act(
            "TEAM_SET_OVERRIDE_RELATION_TO_TEAM",
            vec![text("teamPlayer_1"), text("teamPlayer_2"), number(2)],
        ),
    ];
    if merge {
        actions.push(act(
            "TEAM_MERGE_INTO_TEAM",
            vec![text("teamPlayer_1"), text("teamPlayer_1b")],
        ));
    }
    let mut sim = simulate(
        vec![script("Override", None, actions)],
        &[
            team_def("teamPlayer_1"),
            team_def("teamPlayer_1b"),
            team_def("teamPlayer_2"),
        ],
        None,
    );
    for _ in 0..200 {
        sim.tick().unwrap();
    }
    let ids = sim.rules().ids().unwrap();
    let named = |name: &str| {
        sim.state()
            .store(ids.script_name)
            .unwrap()
            .iter()
            .find(|(_, tag)| tag.0 == name)
            .map(|(entity, _)| entity)
            .unwrap()
    };
    let (a, b) = (named("A"), named("B"));
    (sim, a, b)
}

fn health(sim: &Simulation<RotwkRules>, entity: Entity) -> f32 {
    let ids = sim.rules().ids().unwrap();
    sim.state()
        .get(ids.health, entity)
        .map_or(0.0, |h| h.0.current())
}

#[test]
fn a_team_override_decides_who_a_unit_acquires_and_follows_it_across_a_merge() {
    let (sim, a, b) = fighters(false);
    let (rules, world) = (sim.rules(), sim.state());
    assert_eq!(rules.object_relation(world, a, b), Relation::Allies);
    assert_eq!(rules.object_relation(world, b, a), Relation::Enemies);
    assert_eq!(rules.view_relation(world, P1, b), Relation::Enemies);
    assert_eq!(
        health(&sim, b),
        10_000.0,
        "the overridden side never shoots"
    );
    assert!(health(&sim, a) < 10_000.0, "the other side still does");

    let (merged, _, b) = fighters(true);
    assert!(
        health(&merged, b) < 10_000.0,
        "a unit moved to a team without the override acquires"
    );
}

#[test]
fn a_map_sides_enemy_list_is_one_way_until_a_script_relates_the_other_side() {
    let run = |relate_back: bool| {
        let mut actions = vec![
            spawn("A", "ZFighter", "teamE"),
            spawn("B", "ZFighter", "teamW"),
        ];
        if relate_back {
            actions.push(relate("W", "E", 0));
        }
        let mut sim = simulate_with(
            vec![script("Sides", None, actions)],
            &[team_def("teamE"), team_def("teamW")],
            None,
            &[],
            |rules| {
                rules.with_map_players(
                    2,
                    vec![String::from("W"), String::from("E")],
                    vec![
                        crate::alliance::MapRelations::default(),
                        crate::alliance::MapRelations {
                            enemies: vec![PlayerId::new(2)],
                            ..Default::default()
                        },
                    ],
                )
            },
        );
        for _ in 0..200 {
            sim.tick().unwrap();
        }
        let ids = sim.rules().ids().unwrap();
        let named = |name: &str| {
            sim.state()
                .store(ids.script_name)
                .unwrap()
                .iter()
                .find(|(_, tag)| tag.0 == name)
                .map(|(entity, _)| entity)
                .unwrap()
        };
        (health(&sim, named("A")), health(&sim, named("B")))
    };
    let (e, w) = run(false);
    assert!(w < 10_000.0, "E shoots W");
    assert_eq!(e, 10_000.0, "W names nobody and does not shoot back");
    let (e, _) = run(true);
    assert!(e < 10_000.0, "W shoots E once a script says so");
}

fn counter_of(sim: &Simulation<RotwkRules>, name: &str) -> i32 {
    state_of(sim).counter_value("", name)
}

fn count_when(name: &str, condition: (&str, Vec<Parameter>)) -> Script {
    script(
        name,
        Some(condition),
        vec![act("INCREMENT_COUNTER", vec![number(1), text(name)])],
    )
}

fn live_owner(sim: &Simulation<RotwkRules>, unit: &str) -> Option<PlayerId> {
    let ids = sim.rules().ids().unwrap();
    let world = sim.state();
    let entity = world.store(ids.script_name)?.iter().find_map(|(e, tag)| {
        (tag.0 == unit && world.is_live(e) && !world.contains(ids.doomed, e)).then_some(e)
    })?;
    world.get(ids.owner, entity).map(|o| o.0)
}

#[test]
fn all_destroyed_is_false_inside_the_base_check_grace_and_needs_every_player_empty() {
    let scripts = || {
        vec![
            script("Setup", None, vec![spawn("U", "ZSoldier", "teamWilds")]),
            count_when("empty", ("PLAYER_ALL_DESTROYED", vec![text("Player_1")])),
            count_when("held", ("PLAYER_ALL_DESTROYED", vec![text("Wilds")])),
        ]
    };
    let after_grace = simulate_with(scripts(), &[], None, &[], |r| r.with_base_check_frames(1));
    assert_eq!(counter_of(&after_grace, "empty"), 1);
    assert_eq!(counter_of(&after_grace, "held"), 0, "Wilds still owns U");
    let inside_grace = simulate_with(scripts(), &[], None, &[], |r| {
        r.with_base_check_frames(1_000)
    });
    assert_eq!(counter_of(&inside_grace, "empty"), 0);
}

#[test]
fn player_kill_takes_every_object_of_the_players_in_the_mask() {
    let sim = simulate(
        vec![script(
            "Kill",
            None,
            vec![
                spawn("A", "ZSoldier", "teamWilds"),
                spawn("B", "ZBuilding", "teamWilds"),
                act("PLAYER_KILL", vec![text("Wilds")]),
            ],
        )],
        &[],
        None,
    );
    assert_eq!(live_owner(&sim, "A"), None);
    assert_eq!(live_owner(&sim, "B"), None);
}

#[test]
fn transferring_a_player_moves_its_objects_and_all_its_money_to_the_second_name() {
    let sim = simulate(
        vec![script(
            "Transfer",
            None,
            vec![
                spawn("A", "ZSoldier", "teamWilds"),
                act(
                    "PLAYER_TRANSFER_OWNERSHIP_PLAYER",
                    vec![text("Wilds"), text("Player_1")],
                ),
                act(
                    "PLAYER_TRANSFER_OWNERSHIP_PLAYER",
                    vec![text("Player_1"), text("Player_2")],
                ),
            ],
        )],
        &[],
        None,
    );
    assert_eq!(live_owner(&sim, "A"), Some(P2));
    assert_eq!(purse(&sim, P1), 0);
    assert_eq!(purse(&sim, P2), 2_000);
}

#[test]
fn a_named_transfer_wants_one_player_and_a_team_transfer_takes_the_lowest() {
    let sim = simulate(
        vec![script(
            "Transfer",
            None,
            vec![
                spawn("A", "ZSoldier", "teamWilds"),
                spawn("B", "ZSoldier", "teamWilds"),
                spawn("C", "ZSoldier", "teamPlayer_2"),
                act(
                    "NAMED_TRANSFER_OWNERSHIP_PLAYER",
                    vec![text("A"), text("<All Players>")],
                ),
                act(
                    "NAMED_TRANSFER_OWNERSHIP_PLAYER",
                    vec![text("B"), text("Player_2")],
                ),
                act(
                    "TEAM_TRANSFER_TO_PLAYER",
                    vec![text("teamPlayer_2"), text("<All Players>")],
                ),
            ],
        )],
        &[team_def("teamWilds"), team_def("teamPlayer_2")],
        None,
    );
    assert_eq!(live_owner(&sim, "A"), Some(PlayerId::new(2)));
    assert_eq!(live_owner(&sim, "B"), Some(P2));
    assert_eq!(live_owner(&sim, "C"), Some(P1));
}

#[test]
fn lost_object_type_fires_once_the_stored_count_drops() {
    let sim = simulate(
        vec![
            script("Setup", None, vec![spawn("A", "ZSoldier", "teamWilds")]),
            count_when(
                "lost",
                (
                    "PLAYER_LOST_OBJECT_TYPE",
                    vec![text("Wilds"), text("ZSoldier")],
                ),
            ),
            script("Kill", None, vec![act("NAMED_KILL", vec![text("A")])]),
        ],
        &[],
        None,
    );
    assert_eq!(counter_of(&sim, "lost"), 1);
}

#[test]
fn hunting_marks_the_player() {
    let sim = simulate(
        vec![script(
            "Hunt",
            None,
            vec![
                spawn("A", "ZSoldier", "teamWilds"),
                act("PLAYER_HUNT", vec![text("Wilds")]),
            ],
        )],
        &[],
        None,
    );
    assert!(state_of(&sim).rt.standing.is_hunting(PlayerId::new(2)));
}

#[test]
fn an_unnamed_create_without_that_waypoint_joins_the_team_from_the_nearest_object_of_that_type() {
    let sim = simulate_with(
        vec![script(
            "Create",
            None,
            vec![
                spawn("A", "ZSoldier", "teamWilds"),
                spawn("B", "ZBuilding", "teamPlayer_1"),
                act(
                    "CREATE_UNNAMED_ON_TEAM_AT_WAYPOINT",
                    vec![text("ZArcher"), text("teamWilds"), text("ZBuilding")],
                ),
            ],
        )],
        &[wilds_team("teamWilds"), wilds_team("teamPlayer_1")],
        None,
        &[vec![String::from("Wilds")]],
        |rules| rules,
    );
    let state = state_of(&sim);
    assert!(state.rt.teams.instances().any(|t| t.members().len() == 2));
}

#[test]
fn a_reference_name_names_the_nearest_object_of_the_type() {
    let sim = simulate_with(
        vec![script(
            "Ref",
            None,
            vec![
                spawn("A", "ZSoldier", "teamWilds"),
                spawn("B", "ZArcher", "Player_1/teamPlayer_1"),
                act(
                    "SET_REF_TO_NEREST_TEAM_OF_TYPE_OWNED_BY_PLAYER",
                    vec![
                        text("ZArcher"),
                        text("Player_1"),
                        text("teamWilds"),
                        text("Near"),
                    ],
                ),
            ],
        )],
        &[
            wilds_team("teamWilds"),
            TeamDef {
                owner: String::from("Player_1"),
                ..team_def("teamPlayer_1")
            },
        ],
        None,
        &[vec![String::from("Wilds")]],
        |rules| rules,
    );
    let state = state_of(&sim);
    let archer = named(&sim, "B");
    let near = state
        .rt
        .objects
        .resolve("Near", "Wilds", |_| true, |_| None);
    assert_eq!(near, Some(archer));
}

fn aggressive_team(name: &str, owner: &str) -> TeamDef {
    TeamDef {
        owner: owner.to_owned(),
        properties: Dictionary::from_pairs(vec![(
            String::from("teamAggressiveness"),
            mint_sage_map::Value::Int(2),
        )]),
        ..team_def(name)
    }
}

#[test]
fn a_team_seeds_its_members_attitude_and_only_computer_units_read_it() {
    let sim = simulate_with(
        vec![script(
            "Seed",
            None,
            vec![
                spawn("Cpu", "ZSoldier", "teamWilds"),
                spawn("Human", "ZSoldier", "Player_1/teamPlayer_1"),
                spawn("Slept", "ZSoldier", "teamWilds"),
                act("NAMED_SET_ATTITUDE", vec![text("Slept"), number(-2)]),
            ],
        )],
        &[
            aggressive_team("teamWilds", "Wilds"),
            aggressive_team("teamPlayer_1", "Player_1"),
        ],
        None,
        &[vec![String::from("Wilds")]],
        |rules| rules,
    );
    let ids = sim.rules().ids().unwrap();
    let mood = |name| {
        sim.rules()
            .mood_attitude(sim.state(), ids, named(&sim, name))
    };
    assert_eq!(mood("Cpu"), 2);
    assert_eq!(mood("Human"), 0);
    assert_eq!(mood("Slept"), -2);
    assert!(sim
        .rules()
        .mood_asleep(sim.state(), ids, named(&sim, "Slept")));
    assert_eq!(
        sim.rules()
            .stored_attitude(sim.state(), ids, named(&sim, "Human")),
        2
    );
}

#[test]
fn only_a_unit_holding_a_set_ranks_by_priority_and_a_late_team_member_takes_the_teams() {
    let sim = simulate_with(
        vec![script(
            "Sets",
            None,
            vec![
                spawn("Bare", "ZSoldier", "teamPlayer_1"),
                act("SET_DEFAULT_ATTACK_PRIORITY", vec![text("Set"), number(9)]),
                spawn("Early", "ZSoldier", "teamWilds"),
                act(
                    "TEAM_APPLY_ATTACK_PRIORITY_SET",
                    vec![text("teamWilds"), text("Set")],
                ),
                spawn("Late", "ZSoldier", "teamWilds"),
            ],
        )],
        &[wilds_team("teamWilds"), wilds_team("teamPlayer_1")],
        None,
        &[vec![String::from("Wilds")]],
        |rules| rules,
    );
    let ids = sim.rules().ids().unwrap();
    let default_of = |name| {
        sim.rules()
            .priority_set_of(sim.state(), ids, named(&sim, name))
            .map(|set| set.default)
    };
    assert_eq!(default_of("Bare"), None);
    assert_eq!(default_of("Early"), Some(9));
    assert_eq!(default_of("Late"), Some(9));
}

#[test]
fn a_computer_move_order_skips_sleepers_and_turns_aggressive_units_to_attack_move() {
    let mut sim = simulate_with(
        vec![script(
            "Seed",
            None,
            vec![
                spawn("Cpu", "ZSoldier", "teamWilds"),
                spawn("Slept", "ZSoldier", "teamWilds"),
                act("NAMED_SET_ATTITUDE", vec![text("Slept"), number(-2)]),
            ],
        )],
        &[aggressive_team("teamWilds", "Wilds")],
        None,
        &[vec![String::from("Wilds")]],
        |rules| rules,
    );
    let ids = sim.rules().ids().unwrap();
    let (cpu, slept) = (named(&sim, "Cpu"), named(&sim, "Slept"));
    let wilds = PlayerId::new(2);
    sim.submit(
        wilds,
        crate::rules::RotwkCommand::SetSelection {
            replace: true,
            units: vec![cpu, slept],
        },
    )
    .unwrap();
    sim.submit(
        wilds,
        crate::rules::RotwkCommand::MoveTo {
            x: 300.0,
            y: 300.0,
            queue: false,
        },
    )
    .unwrap();
    sim.tick().unwrap();
    let state = sim.state();
    assert!(state.contains(ids.attack_move_order, cpu));
    assert!(!state.contains(ids.attack_move_order, slept) && !state.contains(ids.path, slept));
}

#[test]
fn a_team_woken_earlier_in_the_same_pass_takes_the_move_order() {
    let sim = simulate_with(
        vec![
            script(
                "Wake",
                Some(("FLAG", vec![text("Spawned"), number(1)])),
                vec![
                    act("TEAM_SET_ATTITUDE", vec![text("teamWilds"), number(0)]),
                    act("MOVE_NAMED_UNIT_TO", vec![text("Sleeper"), text("P2")]),
                ],
            ),
            script(
                "Spawn",
                None,
                vec![
                    spawn("Sleeper", "ZSoldier", "teamWilds"),
                    act("SET_FLAG", vec![text("Spawned"), number(1)]),
                ],
            ),
        ],
        &[TeamDef {
            properties: Dictionary::from_pairs(vec![(
                String::from("teamAggressiveness"),
                mint_sage_map::Value::Int(i32::from(super::ATTITUDE_SLEEP)),
            )]),
            ..aggressive_team("teamWilds", "Wilds")
        }],
        None,
        &[vec![String::from("Wilds")]],
        |rules| rules,
    );
    let ids = sim.rules().ids().unwrap();
    assert!(sim.state().contains(ids.path, named(&sim, "Sleeper")));
}

#[test]
fn a_players_attack_order_reads_its_override_toward_the_targets_team() {
    let mut sim = simulate_with(
        vec![script(
            "Override",
            None,
            vec![
                spawn("A", "ZFighter", "teamPlayer_1"),
                spawn("B", "ZFighter", "teamW"),
                act(
                    "PLAYER_SET_OVERRIDE_RELATION_TO_TEAM",
                    vec![text("Player_1"), text("teamW"), number(0)],
                ),
            ],
        )],
        &[team_def("teamPlayer_1"), team_def("teamW")],
        None,
        &[],
        |rules| {
            rules.with_map_players(
                2,
                vec![String::from("W")],
                vec![crate::alliance::MapRelations {
                    allies: vec![P1],
                    ..Default::default()
                }],
            )
        },
    );
    for _ in 0..10 {
        sim.tick().unwrap();
    }
    let ids = sim.rules().ids().unwrap();
    let (a, b) = (named(&sim, "A"), named(&sim, "B"));
    sim.submit(
        P1,
        crate::rules::RotwkCommand::SetSelection {
            replace: true,
            units: vec![a],
        },
    )
    .unwrap();
    sim.submit(P1, crate::rules::RotwkCommand::AttackObject { target: b })
        .unwrap();
    sim.tick().unwrap();
    assert!(sim.state().contains(ids.attack_order, a));
}

#[test]
fn create_named_joins_the_team_getteamnamed_finds_and_is_owned_by_its_controller() {
    let sim = simulate_with(
        vec![script(
            "Create",
            None,
            vec![
                spawn("A", "ZSoldier", "Wilds/Intro Team"),
                spawn("B", "ZSoldier", "Guards"),
                spawn("C", "ZSoldier", "teamPlayer_1"),
            ],
        )],
        &[
            wilds_team("Intro Team"),
            wilds_team("Guards"),
            wilds_team("teamPlayer_1"),
        ],
        None,
        &[vec![String::from("Wilds")]],
        |rules| rules,
    );
    let state = state_of(&sim);
    let teams: Vec<_> = ["A", "B", "C"]
        .map(|unit| state.rt.teams.team_of(named(&sim, unit)))
        .into();
    assert!(teams.iter().all(Option::is_some), "{teams:?}");
    assert_eq!(
        teams
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
    for unit in ["A", "B", "C"] {
        assert_eq!(live_owner(&sim, unit), Some(PlayerId::new(2)), "{unit}");
    }
}

#[test]
fn a_priority_list_continues_past_a_template_the_match_did_not_load() {
    let list = |names: &[&str]| {
        let mut actions: Vec<_> = names
            .iter()
            .map(|name| act("OBJECTLIST_ADDOBJECTTYPE", vec![text("Group"), text(name)]))
            .collect();
        actions.push(act(
            "SET_ATTACK_PRIORITY_THING",
            vec![text("Set"), text("Group"), number(7)],
        ));
        actions
    };
    let known = std::sync::Arc::new(crate::combat::priority::AttackPriorityData {
        defined_templates: ["zunloaded".to_owned()].into(),
        ..Default::default()
    });
    let sim = simulate_with(
        vec![script(
            "Priority",
            None,
            list(&["ZSoldier", "ZUnloaded", "ZArcher"]),
        )],
        &[],
        None,
        &[],
        |rules| rules.with_attack_priorities(known),
    );
    assert_eq!(
        state_of(&sim).rt.priorities.templates_of("Set"),
        ["zarcher", "zsoldier", "zunloaded"]
    );
}
