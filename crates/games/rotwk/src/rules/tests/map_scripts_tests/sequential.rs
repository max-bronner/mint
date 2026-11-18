use mint_sage_map::ScriptSequence;
use mint_sim::Simulation;

use crate::rules::map_scripts::MapScriptState;

use super::*;

fn make() -> ScriptNode {
    one_shot(
        "Make",
        condition("CONDITION_TRUE", Vec::new()),
        vec![action(
            "CREATE_REINFORCEMENT_TEAM",
            vec![string_param("Squad"), string_param("SpawnPoint_1")],
        )],
    )
}

fn flagged(name: &str, actions: Vec<ScriptAction>, loops: i32) -> ScriptNode {
    let mut seq = script(
        name,
        true,
        true,
        vec![condition("TEAM_HAS_UNITS", vec![string_param("Squad")])],
        actions,
        Vec::new(),
    );
    seq.sequence = ScriptSequence {
        run_sequentially: true,
        loop_count: loops,
        target: String::from("Squad"),
        ..ScriptSequence::default()
    };
    ScriptNode::Script(seq)
}

fn queued(name: &str, actions: Vec<ScriptAction>) -> ScriptNode {
    ScriptNode::Script(script(name, false, false, Vec::new(), actions, Vec::new()))
}

fn bump(counter: &str) -> ScriptAction {
    action(
        "INCREMENT_COUNTER",
        vec![int_param(1), string_param(counter)],
    )
}

fn squad_member_x(simulation: &Simulation<RotwkRules>) -> f32 {
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    world
        .store(ids.owner)
        .and_then(|store| {
            store
                .iter()
                .find(|(entity, owner)| world.is_live(*entity) && owner.0 == PlayerId::new(2))
                .and_then(|(entity, _)| world.get(ids.body, entity))
                .map(|body| body.0.position.x)
        })
        .unwrap_or(0.0)
}

struct Trace {
    changes: Vec<(u32, String, i32)>,
    x_at: Vec<(u32, f32)>,
}

impl Trace {
    fn first(&self, counter: &str, value: i32) -> Option<u32> {
        self.changes
            .iter()
            .find(|(_, name, v)| name == counter && *v >= value)
            .map(|(tick, _, _)| *tick)
    }

    fn x_when(&self, tick: u32) -> f32 {
        self.x_at
            .iter()
            .find(|(t, _)| *t == tick)
            .map_or(0.0, |(_, x)| *x)
    }
}

fn state_of(simulation: &Simulation<RotwkRules>) -> Option<MapScriptState> {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, s)| s.clone()))
}

fn total(state: &MapScriptState, name: &str) -> i32 {
    state.counter_value("Wilds", name) + state.counter_value("", name)
}

fn trace(nodes: Vec<ScriptNode>, units: i32, ticks: u32, counters: &[&str]) -> Trace {
    let mut simulation = squad_sim(nodes, units);
    let mut seen = vec![0; counters.len()];
    let mut out = Trace {
        changes: Vec::new(),
        x_at: Vec::new(),
    };
    for tick in 1..=ticks {
        simulation.tick().unwrap();
        let Some(state) = state_of(&simulation) else {
            continue;
        };
        for (slot, name) in counters.iter().enumerate() {
            let value = total(&state, name);
            if seen[slot] != value {
                seen[slot] = value;
                out.changes.push((tick, (*name).to_owned(), value));
                out.x_at.push((tick, squad_member_x(&simulation)));
            }
        }
    }
    out
}

const SCRIPT_FRAME_TICKS: u32 = mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;

#[test]
fn a_flagged_script_waits_for_the_teams_move_to_end_before_its_next_action() {
    let run = trace(
        vec![
            make(),
            flagged(
                "Seq",
                vec![
                    action(
                        "MOVE_TEAM_TO",
                        vec![string_param("Squad"), string_param("SpawnPoint_2")],
                    ),
                    bump("done"),
                ],
                0,
            ),
        ],
        1,
        600,
        &["done"],
    );
    let tick = run
        .first("done", 1)
        .expect("the action after the move never ran");
    assert!(run.x_when(tick) > 55.0, "ran at x = {}", run.x_when(tick));
}

#[test]
fn a_flagged_script_waits_for_the_teams_attack_move_to_end_before_its_next_action() {
    let run = trace(
        vec![
            make(),
            flagged(
                "Seq",
                vec![
                    action(
                        "ATTACK_MOVE_TEAM_TO",
                        vec![string_param("Squad"), string_param("SpawnPoint_2")],
                    ),
                    bump("done"),
                ],
                0,
            ),
        ],
        1,
        600,
        &["done"],
    );
    let tick = run
        .first("done", 1)
        .expect("the action after the attack-move never ran");
    assert!(run.x_when(tick) > 45.0, "ran at x = {}", run.x_when(tick));
}

#[test]
fn idle_for_seconds_holds_the_next_action_five_script_frames_per_second() {
    let run = trace(
        vec![
            make(),
            flagged(
                "Seq",
                vec![
                    bump("start"),
                    action(
                        "TEAM_IDLE_FOR_SECONDS",
                        vec![string_param("Squad"), int_param(2)],
                    ),
                    bump("done"),
                ],
                0,
            ),
        ],
        2,
        400,
        &["start", "done"],
    );
    let start = run.first("start", 1).unwrap();
    let done = run.first("done", 1).expect("the held action never ran");
    assert_eq!(done - start, 10 * SCRIPT_FRAME_TICKS);
}

#[test]
fn a_second_execute_on_the_same_team_queues_behind_the_first() {
    let run = trace(
        vec![
            make(),
            one_shot(
                "Main",
                condition("TEAM_HAS_UNITS", vec![string_param("Squad")]),
                vec![
                    action(
                        "TEAM_EXECUTE_SEQUENTIAL_SCRIPT",
                        vec![string_param("Squad"), string_param("First")],
                    ),
                    action(
                        "TEAM_EXECUTE_SEQUENTIAL_SCRIPT",
                        vec![string_param("Squad"), string_param("Second")],
                    ),
                ],
            ),
            queued(
                "First",
                vec![
                    action(
                        "TEAM_IDLE_FOR_FRAMECOUNT",
                        vec![string_param("Squad"), int_param(5)],
                    ),
                    bump("first"),
                ],
            ),
            queued("Second", vec![bump("second")]),
        ],
        2,
        400,
        &["first", "second"],
    );
    let first = run.first("first", 1).unwrap();
    let second = run.first("second", 1).expect("the queued script never ran");
    assert!(second >= first, "first at {first}, second at {second}");
}

#[test]
fn a_looping_execute_runs_the_body_the_count_it_names() {
    let run = trace(
        vec![
            make(),
            one_shot(
                "Main",
                condition("TEAM_HAS_UNITS", vec![string_param("Squad")]),
                vec![action(
                    "TEAM_EXECUTE_SEQUENTIAL_SCRIPT_LOOPING",
                    vec![string_param("Squad"), string_param("Body"), int_param(3)],
                )],
            ),
            queued(
                "Body",
                vec![
                    bump("runs"),
                    action(
                        "TEAM_IDLE_FOR_FRAMECOUNT",
                        vec![string_param("Squad"), int_param(2)],
                    ),
                ],
            ),
        ],
        2,
        600,
        &["runs"],
    );
    assert_eq!(run.changes.last().map(|c| c.2), Some(3));
}

#[test]
fn a_flagged_script_whose_conditions_fail_is_deactivated_and_runs_nothing() {
    let mut seq = script(
        "Seq",
        true,
        false,
        vec![condition("TEAM_HAS_UNITS", vec![string_param("Nobody")])],
        vec![bump("then")],
        vec![bump("else")],
    );
    seq.sequence = ScriptSequence {
        run_sequentially: true,
        target: String::from("Squad"),
        ..ScriptSequence::default()
    };
    let mut simulation = squad_sim(vec![make(), ScriptNode::Script(seq)], 2);
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let state = state_of(&simulation).unwrap();
    assert!(!state.script_active(1));
    assert_eq!((total(&state, "then"), total(&state, "else")), (0, 0));
}

#[test]
fn at_most_twenty_one_instant_actions_run_in_one_pass() {
    let run = trace(
        vec![
            make(),
            flagged("Seq", (0..30).map(|_| bump("n")).collect(), 0),
        ],
        2,
        400,
        &["n"],
    );
    let start = run.changes.first().map(|c| c.0).unwrap();
    let in_first_pass = run.changes.iter().rfind(|c| c.0 == start);
    assert_eq!(in_first_pass.map(|c| c.2), Some(21));
    assert_eq!(run.changes.last().map(|c| c.2), Some(30));
}
