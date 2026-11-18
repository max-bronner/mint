use mint_sage_map::{Dictionary, TeamDef, Value};
use mint_sim::Simulation;

use crate::rules::map_scripts::MapScriptState;

use super::*;

fn hits(simulation: &Simulation<RotwkRules>, counter: &str) -> i32 {
    let ids = simulation.rules().ids().unwrap();
    let state: MapScriptState = simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, s)| s.clone()))
        .unwrap();
    state.counter_value("Wilds", counter) + state.counter_value("", counter)
}

fn squad_on_destroyed(threshold: f32) -> TeamDef {
    TeamDef {
        owner: String::from("Wilds"),
        name: String::from("Squad"),
        singleton: true,
        properties: Dictionary::from_pairs(vec![
            (String::from("teamUnitType1"), Value::Str("ZGollum".into())),
            (String::from("teamUnitMaxCount1"), Value::Int(2)),
            (String::from("teamUnitMinCount1"), Value::Int(2)),
            (
                String::from("teamOnDestroyedScript"),
                Value::Str("Gone".into()),
            ),
            (
                String::from("teamDestroyedThreshold"),
                Value::Float(threshold),
            ),
        ]),
    }
}

fn nodes() -> Vec<ScriptNode> {
    let make = one_shot(
        "Make",
        condition("CONDITION_TRUE", Vec::new()),
        vec![action(
            "CREATE_REINFORCEMENT_TEAM",
            vec![string_param("Squad"), string_param("SpawnPoint_1")],
        )],
    );
    let kill = one_shot(
        "Kill",
        condition("TEAM_HAS_UNITS", vec![string_param("Squad")]),
        vec![action("TEAM_KILL", vec![string_param("Squad")])],
    );
    let mut gone = script(
        "Gone",
        true,
        false,
        vec![condition("CONDITION_TRUE", Vec::new())],
        vec![action(
            "INCREMENT_COUNTER",
            vec![int_param(1), string_param("gone")],
        )],
        Vec::new(),
    );
    gone.subroutine = true;
    vec![make, kill, ScriptNode::Script(gone)]
}

#[test]
fn on_destroyed_runs_once_when_the_team_falls_to_its_threshold() {
    let mut simulation =
        squad_sim_with(nodes(), squad_on_destroyed(0.5), WALKER_DATA, &["ZGollum"]);
    for _ in 0..120 {
        simulation.tick().unwrap();
    }
    assert_eq!(hits(&simulation, "gone"), 1);
}
