use mint_sage_shroud::CellVisibility;
use mint_sim::Simulation;

use super::*;

const SPOT: (f32, f32) = (20.0, 20.0);

fn typed(name: &str, action_type: i32, params: Vec<Parameter>) -> ScriptAction {
    ScriptAction {
        action_type,
        ..action(name, params)
    }
}

fn run(actions: Vec<ScriptAction>) -> Simulation<RotwkRules> {
    let one_shot = script(
        "Reveal",
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
        [(String::from("Spot"), (Vec2::new(SPOT.0, SPOT.1), 0.0))]
            .into_iter()
            .collect();
    let areas: BTreeMap<String, Vec<(f32, f32)>> = [(
        String::from("Box"),
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
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    simulation
}

fn at_spot<R>(
    simulation: &Simulation<RotwkRules>,
    read: impl FnOnce(&mint_sage_shroud::VisibilityGrid, i32, i32) -> R,
) -> R {
    let ids = simulation.rules().ids().unwrap();
    crate::shroud::with_grid(simulation.state(), RotwkRules::shroud_ids(ids), |grid| {
        let (x, y) = grid.world_to_cell(SPOT.0, SPOT.1);
        read(grid, x, y)
    })
    .unwrap()
}

fn lookers(simulation: &Simulation<RotwkRules>, player: u8) -> u32 {
    at_spot(simulation, |grid, x, y| {
        grid.looker_count(PlayerId::new(player), x, y)
    })
}

fn cell(simulation: &Simulation<RotwkRules>, player: u8) -> CellVisibility {
    at_spot(simulation, |grid, x, y| {
        grid.cell_state(PlayerId::new(player), x, y)
    })
}

fn shroud_everything() -> ScriptAction {
    action("MAP_SHROUD_ALL", vec![string_param("")])
}

fn waypoint_reveal(name: &str) -> ScriptAction {
    action(
        "MAP_REVEAL_PERMANENTLY_AT_WAYPOINT",
        vec![
            string_param("Spot"),
            real_param(4.0),
            string_param("Player_1"),
            string_param(name),
        ],
    )
}

fn trigger_reveal(name: &str) -> ScriptAction {
    action(
        "MAP_REVEAL_PERMANENTLY_IN_TRIGGER",
        vec![
            string_param("Box"),
            string_param("Player_1"),
            string_param(name),
        ],
    )
}

#[test]
fn an_undo_in_trigger_undoes_a_waypoint_reveal_of_that_name() {
    let kept = run(vec![waypoint_reveal("R")]);
    assert!(lookers(&kept, 0) > 0);
    for undo in [
        "MAP_UNDO_REVEAL_PERMANENTLY_IN_TRIGGER",
        "MAP_UNDO_REVEAL_PERMANENTLY_AT_WAYPOINT",
    ] {
        let undone = run(vec![
            waypoint_reveal("R"),
            action(undo, vec![string_param("R")]),
        ]);
        assert_eq!(lookers(&undone, 0), 0, "{undo}");
    }
    let trigger = run(vec![
        trigger_reveal("T"),
        action(
            "MAP_UNDO_REVEAL_PERMANENTLY_AT_WAYPOINT",
            vec![string_param("T")],
        ),
    ]);
    assert_eq!(lookers(&trigger, 0), 0);
}

#[test]
fn a_second_reveal_of_a_name_in_use_is_refused_and_an_unknown_undo_does_nothing() {
    let simulation = run(vec![
        waypoint_reveal("R"),
        trigger_reveal("R"),
        action(
            "MAP_UNDO_REVEAL_PERMANENTLY_IN_TRIGGER",
            vec![string_param("Other")],
        ),
    ]);
    assert!(lookers(&simulation, 0) > 0);
}

#[test]
fn a_look_leaves_the_area_explored_and_an_unknown_player_sees_nothing() {
    let looked = run(vec![
        shroud_everything(),
        action(
            "MAP_REVEAL_AT_WAYPOINT",
            vec![
                string_param("Spot"),
                real_param(4.0),
                string_param("Player_1"),
            ],
        ),
    ]);
    assert_eq!(lookers(&looked, 0), 0);
    assert_eq!(cell(&looked, 0), CellVisibility::Explored);
    let nobody = run(vec![
        shroud_everything(),
        action(
            "MAP_REVEAL_AT_WAYPOINT",
            vec![
                string_param("Spot"),
                real_param(4.0),
                string_param("Nobody"),
            ],
        ),
    ]);
    for player in [0, 1] {
        assert_eq!(cell(&nobody, player), CellVisibility::Unexplored);
    }
}

#[test]
fn a_trigger_look_runs_as_stored_id_552_and_does_nothing_as_553() {
    let params = || vec![string_param("Box"), string_param("Player_1")];
    let run552 = run(vec![
        shroud_everything(),
        typed("MAP_REVEAL_IN_TRIGGER", 552, params()),
    ]);
    assert_eq!(cell(&run552, 0), CellVisibility::Explored);
    let run553 = run(vec![
        shroud_everything(),
        typed("MAP_REVEAL_IN_TRIGGER", 553, params()),
    ]);
    assert_eq!(cell(&run553, 0), CellVisibility::Unexplored);
}

#[test]
fn whole_map_verbs_default_to_every_human_and_undo_what_they_did() {
    let revealed = run(vec![action("MAP_REVEAL_ALL_PERM", vec![string_param("")])]);
    assert!(lookers(&revealed, 0) > 0 && lookers(&revealed, 1) > 0);
    let one = run(vec![action(
        "MAP_REVEAL_ALL_PERM",
        vec![string_param("Player_1")],
    )]);
    assert!(lookers(&one, 0) > 0);
    assert_eq!(lookers(&one, 1), 0);
    let undone = run(vec![
        action("MAP_REVEAL_ALL_PERM", vec![string_param("")]),
        action("MAP_REVEAL_ALL_UNDO_PERM", vec![string_param("")]),
    ]);
    assert_eq!(lookers(&undone, 0), 0);
    let glimpsed = run(vec![action("MAP_REVEAL_ALL", vec![string_param("")])]);
    assert_eq!(cell(&glimpsed, 1), CellVisibility::Explored);
    let reshrouded = run(vec![
        action("MAP_REVEAL_ALL", vec![string_param("")]),
        action("MAP_SHROUD_ALL", vec![string_param("")]),
    ]);
    assert_eq!(cell(&reshrouded, 0), CellVisibility::Unexplored);
}
