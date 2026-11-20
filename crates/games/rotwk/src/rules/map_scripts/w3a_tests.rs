use std::collections::BTreeMap;
use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::{
    Condition, Dictionary, HeightMapData, OrCondition, Parameter, Script, ScriptAction, ScriptNode,
    ScriptTree, TeamDef,
};
use mint_sage_passability::Passability;
use mint_sim::{Entity, PlayerId, Simulation};

use super::team::TeamProtos;
use super::{MapScriptEngine, MapScriptState};
use crate::rules::RotwkRules;
use crate::templates::KindTable;

pub(super) const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
ExperienceLevel ZLevel1\n  TargetNames = ZHero\n  RequiredExperience = 1\n  Rank = 1\nEnd\n\
ExperienceLevel ZLevel2\n  TargetNames = ZHero\n  RequiredExperience = 100\n  Rank = 2\nEnd\n\
ExperienceLevel ZLevel3\n  TargetNames = ZHero\n  RequiredExperience = 1000\n  Rank = 3\nEnd\n\
Object ZHero\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE HERO\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n\
Object ZLight\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n\
Object ZHeavy\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  ShockwaveResistance = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n";

pub(super) fn text(value: &str) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: 0.0,
        string_value: value.to_owned(),
    }
}

fn status(name: &str) -> Parameter {
    Parameter::Value {
        param_type: 41,
        int_value: 0,
        real_value: 0.0,
        string_value: name.to_owned(),
    }
}

pub(super) fn number(value: i32) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: value,
        real_value: 0.0,
        string_value: String::new(),
    }
}

pub(super) fn real(value: f32) -> Parameter {
    Parameter::Value {
        param_type: 1,
        int_value: 0,
        real_value: value,
        string_value: String::new(),
    }
}

pub(super) fn act(name: &str, params: Vec<Parameter>) -> ScriptAction {
    ScriptAction {
        action_type: 0,
        enabled: true,
        name: name.to_owned(),
        params,
    }
}

pub(super) fn script(
    name: &str,
    condition: Option<(&str, Vec<Parameter>)>,
    actions: Vec<ScriptAction>,
    one_shot: bool,
) -> Script {
    let (condition_name, params) = condition.unwrap_or(("CONDITION_TRUE", Vec::new()));
    Script {
        name: name.to_owned(),
        comment: String::new(),
        condition_comment: String::new(),
        action_comment: String::new(),
        active: true,
        one_shot,
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

pub(super) fn spawn(unit: &str, template: &str) -> ScriptAction {
    act(
        "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
        vec![text(unit), text(template), text("teamPlayer_1"), text("P1")],
    )
}

pub(super) fn once(name: &str, actions: Vec<ScriptAction>) -> Script {
    script(name, None, actions, true)
}

pub(super) fn after(name: &str, unit: &str, actions: Vec<ScriptAction>) -> Script {
    script(
        name,
        Some(("NAMED_CREATED", vec![text(unit)])),
        actions,
        true,
    )
}

pub(super) fn counting(name: &str, condition: (&str, Vec<Parameter>)) -> Script {
    script(
        name,
        Some(condition),
        vec![act("INCREMENT_COUNTER", vec![number(1), text(name)])],
        true,
    )
}

pub(super) fn simulate(scripts: Vec<Script>, ticks: u32) -> Simulation<RotwkRules> {
    simulate_data(DATA, scripts, ticks)
}

pub(super) fn simulate_data(
    text: &str,
    scripts: Vec<Script>,
    ticks: u32,
) -> Simulation<RotwkRules> {
    let data = crate::game::tests::data_from(text);
    let names = ["ZHero", "ZLight", "ZHeavy"].map(String::from);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: BTreeMap<String, (Vec2, f32)> =
        [(String::from("P1"), (Vec2::new(10.0, 10.0), 0.0))].into();
    let tree = ScriptTree {
        player_slots: vec![scripts.into_iter().map(ScriptNode::Script).collect()],
    };
    let team = TeamDef {
        owner: String::new(),
        name: String::from("teamPlayer_1"),
        singleton: true,
        properties: Dictionary::from_pairs(Vec::new()),
    };
    let engine = MapScriptEngine::build(&[tree], &[], &waypoints, &BTreeMap::new(), false)
        .with_team_prototypes(TeamProtos::from_defs(&[team]).unwrap());
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
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_experience(Arc::clone(&data.experience), data.armor_bonus_cap)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    run(&mut simulation, ticks);
    simulation
}

pub(super) fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

pub(super) fn state_of(simulation: &Simulation<RotwkRules>) -> MapScriptState {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, state)| state.clone()))
        .unwrap()
}

pub(super) fn counter_of(sim: &Simulation<RotwkRules>, name: &str) -> i32 {
    state_of(sim).counter_value("", name)
}

pub(super) fn named(simulation: &Simulation<RotwkRules>, name: &str) -> Entity {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.script_name)
        .and_then(|store| {
            store
                .iter()
                .find(|(entity, tag)| tag.0 == name && simulation.state().is_live(*entity))
                .map(|(entity, _)| entity)
        })
        .unwrap_or_else(|| panic!("{name} was not created"))
}

#[test]
fn a_status_set_on_a_team_is_read_by_the_unit_and_team_conditions() {
    let sim = simulate(
        vec![
            once("Make", vec![spawn("U", "ZLight")]),
            after(
                "Set",
                "U",
                vec![act(
                    "TEAM_CHANGE_OBJECT_STATUS",
                    vec![text("teamPlayer_1"), status("holding_the_shard"), number(1)],
                )],
            ),
            counting(
                "unit",
                (
                    "UNIT_HAS_OBJECT_STATUS",
                    vec![text("U"), status("HOLDING_THE_SHARD")],
                ),
            ),
            counting(
                "team",
                (
                    "TEAM_SOME_HAVE_OBJECT_STATUS",
                    vec![text("teamPlayer_1"), status("HOLDING_THE_SHARD")],
                ),
            ),
            counting(
                "other",
                (
                    "UNIT_HAS_OBJECT_STATUS",
                    vec![text("U"), status("NOT_A_STATUS")],
                ),
            ),
        ],
        60,
    );
    assert_eq!(counter_of(&sim, "unit"), 1);
    assert_eq!(counter_of(&sim, "team"), 1);
    assert_eq!(counter_of(&sim, "other"), 0);
}

#[test]
fn the_indestructible_panel_flag_sets_and_clears_the_body() {
    let flag = |on| {
        vec![
            once("Make", vec![spawn("U", "ZLight")]),
            after(
                "Flag",
                "U",
                vec![act(
                    "UNIT_AFFECT_OBJECT_PANEL_FLAGS",
                    vec![text("U"), text("Indestructible"), number(on)],
                )],
            ),
        ]
    };
    let indestructible = |sim: &Simulation<RotwkRules>| {
        let ids = sim.rules().ids().unwrap();
        sim.state()
            .get(ids.health, named(sim, "U"))
            .unwrap()
            .0
            .is_indestructible()
    };
    assert!(indestructible(&simulate(flag(1), 40)));
    assert!(!indestructible(&simulate(flag(0), 40)));
}

fn points_of(sim: &Simulation<RotwkRules>, unit: &str) -> u32 {
    let ids = sim.rules().ids().unwrap();
    sim.rules()
        .experience_of(sim.state(), ids, named(sim, unit))
        .unwrap()
        .1
}

#[test]
fn awards_stop_at_the_script_level_cap_and_the_level_name_follows_the_points() {
    let give = |amount| {
        act(
            "UNIT_GIVE_EXPERIENCE_POINTS",
            vec![text("U"), number(amount)],
        )
    };
    let sim = simulate(
        vec![
            once("Make", vec![spawn("U", "ZHero")]),
            after("First", "U", vec![give(150)]),
            counting(
                "level2",
                ("UNIT_IS_AT_LEVEL", vec![text("U"), text("ZLevel2")]),
            ),
        ],
        60,
    );
    assert_eq!(points_of(&sim, "U"), 151);
    assert_eq!(counter_of(&sim, "level2"), 1);
    let capped = simulate(
        vec![
            once("Make", vec![spawn("U", "ZHero")]),
            after(
                "Cap",
                "U",
                vec![act("UNIT_SET_MAX_LEVEL", vec![text("U"), number(1)])],
            ),
            after("Give", "U", vec![give(150)]),
        ],
        60,
    );
    assert_eq!(
        points_of(&capped, "U"),
        1,
        "the cap holds a rank-1 hero at its start"
    );
}

#[test]
fn counter_math_applies_the_op_and_a_zero_divisor_changes_nothing() {
    let math = |op, source: &str| {
        act(
            "COUNTER_MATH_COUNTER",
            vec![text("a"), number(op), text(source)],
        )
    };
    let sim = simulate(
        vec![once(
            "Run",
            vec![
                act("SET_COUNTER", vec![text("a"), number(7)]),
                act("SET_COUNTER", vec![text("b"), number(2)]),
                math(2, "b"),
                math(1, "b"),
                math(3, "zero"),
                math(0, "b"),
            ],
        )],
        20,
    );
    assert_eq!(counter_of(&sim, "a"), 14);
}

#[test]
fn an_ownership_counter_waits_for_the_first_object_event_after_frame_zero() {
    let mut sim = simulate(
        vec![
            once(
                "Early",
                vec![
                    spawn("U", "ZLight"),
                    act("SET_TIMER", vec![text("late"), number(3)]),
                ],
            ),
            script(
                "Count",
                None,
                vec![act(
                    "SET_PLAYER_OWNERSHIP_OF_TYPE_COUNTER",
                    vec![text("ZLight"), text("Player_1"), text("owned")],
                )],
                false,
            ),
            script(
                "Late",
                Some(("TIMER_EXPIRED", vec![text("late")])),
                vec![spawn("V", "ZLight")],
                true,
            ),
        ],
        12,
    );
    assert_eq!(counter_of(&sim, "owned"), 0);
    run(&mut sim, 60);
    assert!(counter_of(&sim, "owned") >= 1);
}

#[test]
fn a_scripted_shock_throws_a_light_unit_and_not_a_resistant_one() {
    let shocked = |template: &str| {
        let sim = simulate(
            vec![
                once("Make", vec![spawn("U", template)]),
                after(
                    "Shock",
                    "U",
                    vec![act(
                        "NAMED_SHOCK",
                        vec![text("U"), real(0.0), real(10.0), real(1.0), text("NONE")],
                    )],
                ),
            ],
            20,
        );
        let ids = sim.rules().ids().unwrap();
        sim.state().contains(ids.knocked, named(&sim, "U"))
    };
    assert!(shocked("ZLight"));
    assert!(!shocked("ZHeavy"));
}
