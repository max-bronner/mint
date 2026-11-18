use mint_sage_map::{Dictionary, ScriptSequence, TeamDef, Value};
use mint_sim::Simulation;

use crate::rules::map_scripts::MapScriptState;

use super::*;

fn state_of(simulation: &Simulation<RotwkRules>) -> MapScriptState {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, s)| s.clone()))
        .unwrap()
}

fn hits(simulation: &Simulation<RotwkRules>, counter: &str) -> i32 {
    let state = state_of(simulation);
    state.counter_value("Wilds", counter) + state.counter_value("", counter)
}

fn bump(counter: &str) -> ScriptAction {
    action(
        "INCREMENT_COUNTER",
        vec![int_param(1), string_param(counter)],
    )
}

fn always() -> OrCondition {
    condition("CONDITION_TRUE", Vec::new())
}

fn run(mut simulation: Simulation<RotwkRules>, ticks: u32) -> Simulation<RotwkRules> {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    simulation
}

fn named(simulation: &Simulation<RotwkRules>, name: &str) -> Entity {
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    world
        .store(ids.script_name)
        .and_then(|store| {
            store
                .iter()
                .find(|(entity, tag): &(Entity, &ScriptObjectName)| {
                    tag.0 == name && world.is_live(*entity)
                })
                .map(|(entity, _)| entity)
        })
        .unwrap()
}

#[test]
fn a_subroutine_runs_only_when_a_script_calls_it() {
    let mut sub = script(
        "Sub",
        true,
        false,
        vec![always()],
        vec![bump("sub")],
        Vec::new(),
    );
    sub.subroutine = true;
    let caller = one_shot(
        "Caller",
        always(),
        vec![action("CALL_SUBROUTINE", vec![string_param("Sub")])],
    );
    let simulation = run(squad_sim(vec![ScriptNode::Script(sub), caller], 1), 60);
    assert_eq!(hits(&simulation, "sub"), 1);
}

#[test]
fn a_side_qualified_name_enables_the_script_of_that_side() {
    let later = ScriptNode::Script(script(
        "Later",
        false,
        true,
        vec![always()],
        vec![bump("later")],
        Vec::new(),
    ));
    let enable = one_shot(
        "Enable",
        always(),
        vec![action("ENABLE_SCRIPT", vec![string_param("Wilds/Later")])],
    );
    let simulation = run(squad_sim(vec![enable, later], 1), 60);
    assert_eq!(hits(&simulation, "later"), 1);
}

fn make() -> ScriptNode {
    one_shot(
        "Make",
        always(),
        vec![action(
            "CREATE_REINFORCEMENT_TEAM",
            vec![string_param("Squad"), string_param("SpawnPoint_1")],
        )],
    )
}

fn squad_with_hooks(hooks: &[&str]) -> TeamDef {
    let mut pairs = vec![
        (String::from("teamUnitType1"), Value::Str("ZGollum".into())),
        (String::from("teamUnitMaxCount1"), Value::Int(2)),
        (String::from("teamUnitMinCount1"), Value::Int(1)),
    ];
    for (slot, hook) in hooks.iter().enumerate() {
        pairs.push((
            format!("teamGenericScriptHook{slot}"),
            Value::Str((*hook).to_owned()),
        ));
    }
    TeamDef {
        owner: String::from("Wilds"),
        name: String::from("Squad"),
        singleton: true,
        properties: Dictionary::from_pairs(pairs),
    }
}

fn generic(name: &str, one_shot: bool, flagged: bool, actions: Vec<ScriptAction>) -> ScriptNode {
    let mut hook = script(
        name,
        true,
        one_shot,
        vec![condition(
            "TEAM_HAS_UNITS",
            vec![string_param("<This Team>")],
        )],
        actions,
        Vec::new(),
    );
    hook.subroutine = true;
    if flagged {
        hook.sequence = ScriptSequence {
            run_sequentially: true,
            target: String::from("<This Team>"),
            ..ScriptSequence::default()
        };
    }
    ScriptNode::Script(hook)
}

fn squad_generic(nodes: Vec<ScriptNode>, hooks: &[&str]) -> Simulation<RotwkRules> {
    squad_sim_with(nodes, squad_with_hooks(hooks), WALKER_DATA, &["ZGollum"])
}

#[test]
fn a_generic_one_shot_runs_once_for_its_team_with_the_team_as_this_team() {
    let simulation = run(
        squad_generic(
            vec![make(), generic("Gen", true, false, vec![bump("gen")])],
            &["Gen"],
        ),
        60,
    );
    assert_eq!(hits(&simulation, "gen"), 1);
}

#[test]
fn a_flagged_generic_script_is_queued_on_its_team_only_when_one_shot() {
    let simulation = run(
        squad_generic(
            vec![
                make(),
                generic("Once", true, true, vec![bump("queued")]),
                generic("Every", false, true, vec![bump("never")]),
            ],
            &["Once", "Every"],
        ),
        60,
    );
    assert_eq!(hits(&simulation, "queued"), 1);
    assert_eq!(hits(&simulation, "never"), 0);
}

#[test]
fn a_unit_sequence_resolves_this_object_to_its_unit() {
    let walker = one_shot("Make", always(), vec![named_at("Walker", "SpawnPoint_1")]);
    let mut seq = script(
        "Seq",
        true,
        true,
        vec![always()],
        vec![action(
            "NAMED_SET_ATTITUDE",
            vec![string_param("<This Object>"), int_param(-2)],
        )],
        Vec::new(),
    );
    seq.sequence = ScriptSequence {
        run_sequentially: true,
        target_is_unit: true,
        target: String::from("Walker"),
        ..ScriptSequence::default()
    };
    let simulation = run(squad_sim(vec![walker, ScriptNode::Script(seq)], 1), 30);
    let unit = named(&simulation, "Walker");
    assert_eq!(state_of(&simulation).rt.attitudes.get(unit), -2);
}

#[test]
fn named_kill_resolves_this_object() {
    let walker = one_shot("Make", always(), vec![named_at("Walker", "SpawnPoint_1")]);
    let mut seq = script(
        "Seq",
        true,
        true,
        vec![always()],
        vec![action("NAMED_KILL", vec![string_param("<This Object>")])],
        Vec::new(),
    );
    seq.sequence = ScriptSequence {
        run_sequentially: true,
        target_is_unit: true,
        target: String::from("Walker"),
        ..ScriptSequence::default()
    };
    let simulation = run(squad_sim(vec![walker, ScriptNode::Script(seq)], 1), 30);
    assert!(state_of(&simulation).ever_existed.contains("Walker"));
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let alive = world.store(ids.script_name).is_some_and(|store| {
        store
            .iter()
            .any(|(entity, tag): (Entity, &ScriptObjectName)| {
                tag.0 == "Walker" && world.is_live(entity) && !world.contains(ids.doomed, entity)
            })
    });
    assert!(!alive, "the walker named by <This Object> was not killed");
}

const POST_DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 200\nEnd\n\
Object ZGollum\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n\
Object ZPost\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = ZPostSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    NaturalRallyPoint = X:50 Y:0 Z:0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet ZPostSet\n  1 = Build_ZGollum\nEnd\n\
CommandButton Build_ZGollum\n  Command = UNIT_BUILD\n  Object = ZGollum\nEnd\n";

#[test]
fn a_script_created_production_structure_can_produce() {
    let post = one_shot(
        "Make",
        always(),
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("Post"),
                string_param("ZPost"),
                string_param("teamWilds"),
                string_param("SpawnPoint_2"),
            ],
        )],
    );
    let simulation = run(
        squad_sim_with(
            vec![post],
            squad_with_hooks(&[]),
            POST_DATA,
            &["ZGollum", "ZPost"],
        ),
        12,
    );
    let ids = simulation.rules().ids().unwrap();
    let producer = simulation
        .state()
        .get(ids.producer, named(&simulation, "Post"));
    assert!(producer.is_some_and(|p| p.menu.is_some()));
}

const HORDE_DATA: &str = "Object Raider
  Side = Men
  KindOf = INFANTRY SELECTABLE
    Body = ActiveBody Tag_Body
    MaxHealth = 100
  End
    Behavior = SlowDeathBehavior Tag_Death
    DeathTypes = ALL
    DestructionDelay = 5000
  End
    Geometry = CYLINDER
  GeometryMajorRadius = 5
  GeometryHeight = 16
End
Object RaiderHorde
  Side = Men
  KindOf = SELECTABLE HORDE
  CommandSet = EmptySet
    Behavior = HordeContain Tag_01
    Slots = 2
        RankInfo = RankNumber:1 UnitType:Raider Position:X:20 Y:-20 Position:X:20 Y:20
    End
End
CommandSet EmptySet
End
";

#[test]
fn a_horde_team_whose_soldiers_die_is_destroyed_for_its_generic_hook() {
    let team = TeamDef {
        owner: String::from("Wilds"),
        name: String::from("Squad"),
        singleton: false,
        properties: Dictionary::from_pairs(vec![
            (
                String::from("teamUnitType1"),
                Value::Str("RaiderHorde".into()),
            ),
            (String::from("teamUnitMaxCount1"), Value::Int(1)),
            (String::from("teamUnitMinCount1"), Value::Int(1)),
            (
                String::from("teamGenericScriptHook0"),
                Value::Str("Wiped".into()),
            ),
        ]),
    };
    let mut wiped = script(
        "Wiped",
        true,
        true,
        vec![condition(
            "TEAM_DESTROYED",
            vec![string_param("<This Team>")],
        )],
        vec![bump("wiped")],
        Vec::new(),
    );
    wiped.subroutine = true;
    let mut simulation = run(
        squad_sim_with(
            vec![make(), ScriptNode::Script(wiped)],
            team,
            HORDE_DATA,
            &["RaiderHorde"],
        ),
        60,
    );
    simulation
        .submit(
            PlayerId::new(0),
            RotwkCommand::Debug(crate::rules::DebugCommand::Kill {
                target: crate::rules::DebugTarget::Player(String::from("Wilds")),
            }),
        )
        .unwrap();
    let simulation = run(simulation, 60);
    assert_eq!(hits(&simulation, "wiped"), 1);
}
