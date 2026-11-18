use mint_movement::Vec2;
use mint_sage_map::{ScriptAction, Value};
use mint_sim::{Entity, Simulation};

use super::p67_tests::{rig, team, P0, SIDE};
use super::team::TeamId;
use super::w3a_tests::{act, after, named, number, once, real, run, state_of, text};
use super::PathPoint;
use crate::rules::RotwkRules;

fn make(unit: &str, template: &str, team: &str, waypoint: &str) -> Vec<ScriptAction> {
    vec![
        act(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                text(unit),
                text(template),
                text("teamWilds"),
                text(waypoint),
            ],
        ),
        act("UNIT_SET_TEAM", vec![text(unit), text(team)]),
    ]
}

fn team_of(sim: &Simulation<RotwkRules>, unit: &str) -> Option<TeamId> {
    state_of(sim).rt.teams.team_of(named(sim, unit))
}

fn position_of(sim: &Simulation<RotwkRules>, unit: Entity) -> Vec2 {
    let ids = sim.rules().ids().unwrap();
    sim.state().get(ids.body, unit).unwrap().0.position
}

#[test]
fn recruiting_units_from_a_team_takes_the_nearest_to_a_centre_fixed_once() {
    let mut setup = rig(vec![
        once(
            "Make",
            [
                make("UD", "ZSoldier", "teamDst", "D"),
                make("UA", "ZSoldier", "teamSrc", "A"),
                make("UB", "ZSoldier", "teamSrc", "B"),
                make("UC", "ZSoldier", "teamSrc", "C"),
            ]
            .concat(),
        ),
        after(
            "Recruit",
            "UC",
            vec![act(
                "TEAM_RECRUIT_UNITS_FROM_TEAM",
                vec![
                    text("teamDst"),
                    number(2),
                    text("ZSoldier"),
                    text("teamSrc"),
                ],
            )],
        ),
    ]);
    setup.waypoints = vec![
        ("D", 100.0, 50.0),
        ("A", 110.0, 50.0),
        ("B", 88.0, 50.0),
        ("C", 120.0, 50.0),
    ];
    setup.teams = vec![
        team("Wilds", "teamDst", true, &[]),
        team("Wilds", "teamSrc", true, &[]),
    ];
    let mut sim = setup.start();
    run(&mut sim, 60);
    let dest = team_of(&sim, "UD");
    assert!(dest.is_some());
    assert_eq!(team_of(&sim, "UA"), dest);
    assert_eq!(team_of(&sim, "UB"), dest);
    assert_ne!(team_of(&sim, "UC"), dest);
}

#[test]
fn moving_to_the_nearest_object_of_a_type_ignores_who_owns_it() {
    let mut setup = rig(vec![
        once("Make", make("U", "ZWalker", "teamMover", "P1")),
        after(
            "Go",
            "U",
            vec![act(
                "TEAM_MOVE_TO_NEAREST_OBJECT_OF_TYPE",
                vec![text("teamMover"), text("ZTower")],
            )],
        ),
    ]);
    setup.teams = vec![team("Wilds", "teamMover", true, &[])];
    setup.placed = vec![("ZTower", 40.0, 40.0, SIDE), ("ZTower", 300.0, 300.0, P0)];
    let mut sim = setup.start();
    run(&mut sim, 20);
    let ids = sim.rules().ids().unwrap();
    let goal = sim.state().get(ids.path, named(&sim, "U")).unwrap().goal;
    assert_eq!((goal.x, goal.y), (40.0, 40.0));
}

fn burning_cells(amount: i32) -> usize {
    let mut setup = rig(vec![once(
        "Burn",
        vec![act(
            "CHANGE_BURN_RATE_AT_WAYPOINT",
            vec![text("P2"), real(10.0), number(amount)],
        )],
    )]);
    setup.fire = true;
    let mut sim = setup.start();
    run(&mut sim, 12);
    let ids = sim.rules().ids().unwrap();
    let grid = crate::rules::terrain_fire::singleton_of(sim.state(), ids.fire_grid).unwrap();
    sim.state()
        .get(ids.fire_grid, grid)
        .unwrap()
        .burning_count()
}

#[test]
fn changing_a_burn_rate_lights_the_disc_and_a_zero_amount_changes_nothing() {
    assert!(burning_cells(5) > 0);
    assert_eq!(burning_cells(0), 0);
}

const RECRUITS: usize = 1;

fn recruiting(anchor: bool, soldiers: bool) -> Simulation<RotwkRules> {
    let mut setup = rig(vec![once(
        "Make",
        [
            make("UW", "ZWalker", "teamAnchor", "Far"),
            vec![act(
                "RECRUIT_TEAM_AT_TEAM",
                vec![
                    text("teamRecruits"),
                    real(50.0),
                    text(if anchor { "teamAnchor" } else { "" }),
                ],
            )],
        ]
        .concat(),
    )]);
    setup.waypoints = vec![("Home", 100.0, 50.0), ("Far", 300.0, 50.0)];
    setup.teams = vec![
        team("Wilds", "teamAnchor", true, &[]),
        team(
            "Wilds",
            "teamRecruits",
            false,
            &[
                ("teamHome", Value::Str("Home".to_owned())),
                ("teamUnitType1", Value::Str("ZSoldier".to_owned())),
                ("teamUnitMinCount1", Value::Int(1)),
                ("teamUnitMaxCount1", Value::Int(1)),
            ],
        ),
    ];
    if soldiers {
        setup.placed = vec![
            ("ZSoldier", 100.0, 50.0, SIDE),
            ("ZSoldier", 300.0, 50.0, SIDE),
        ];
    }
    setup.computer = true;
    let mut sim = setup.start();
    run(&mut sim, 300);
    sim
}

fn recruits_at(sim: &Simulation<RotwkRules>) -> Vec<f32> {
    let state = state_of(sim);
    state
        .rt
        .teams
        .instances_of(RECRUITS)
        .into_iter()
        .filter_map(|team| state.rt.teams.get(team))
        .flat_map(|team| team.members().iter().copied())
        .map(|member| position_of(sim, member).x)
        .collect()
}

#[test]
fn recruiting_a_team_at_a_team_gathers_around_that_teams_centre() {
    let sim = recruiting(true, true);
    let at = recruits_at(&sim);
    assert!(
        matches!(at.as_slice(), [x] if (x - 300.0).abs() < 20.0),
        "{at:?}"
    );
    let state = state_of(&sim);
    let instances = state.rt.teams.instances_of(RECRUITS);
    assert!(instances
        .iter()
        .all(|team| state.rt.teams.get(*team).is_some_and(|t| t.active)));
    let at = recruits_at(&recruiting(false, true));
    assert!(
        matches!(at.as_slice(), [x] if (x - 100.0).abs() < 20.0),
        "{at:?}"
    );
}

#[test]
fn recruiting_nobody_deletes_the_new_team() {
    let sim = recruiting(false, false);
    let state = state_of(&sim);
    assert_eq!(state.rt.teams.instances_of(RECRUITS).len(), 0);
    assert!(state.rt.builders.get(SIDE).unwrap().ready_queue.is_empty());
}

#[test]
fn stop_and_disband_idles_the_team_and_merges_it_into_the_default_team() {
    let mut setup = rig(vec![
        once("Make", make("U", "ZWalker", "teamGroup", "P1")),
        after(
            "Go",
            "U",
            vec![
                act("MOVE_TEAM_TO", vec![text("teamGroup"), text("P2")]),
                act("TEAM_STOP_AND_DISBAND", vec![text("teamGroup")]),
            ],
        ),
    ]);
    setup.teams = vec![
        team("Wilds", "teamWilds", true, &[]),
        team("Wilds", "teamGroup", true, &[]),
    ];
    let mut sim = setup.start();
    run(&mut sim, 30);
    let ids = sim.rules().ids().unwrap();
    assert!(!sim.state().contains(ids.path, named(&sim, "U")));
    let engine = sim.rules().map_scripts.as_ref().unwrap();
    let default = engine.team_prototypes().find("Wilds", "teamWilds").unwrap();
    let default = state_of(&sim).rt.teams.instances_of(default)[0];
    assert_eq!(team_of(&sim, "U"), Some(default));
}

#[test]
fn wander_in_place_walks_a_member_about_its_origin() {
    let mut setup = rig(vec![
        once("Make", make("U", "ZWalker", "teamGroup", "P2")),
        after(
            "Go",
            "U",
            vec![act("TEAM_WANDER_IN_PLACE", vec![text("teamGroup")])],
        ),
    ]);
    setup.teams = vec![team("Wilds", "teamGroup", true, &[])];
    let mut sim = setup.start();
    run(&mut sim, 12);
    let unit = named(&sim, "U");
    let mut farthest = 0.0_f32;
    for _ in 0..600 {
        run(&mut sim, 1);
        let at = position_of(&sim, unit);
        let (dx, dy) = (at.x - 60.0, at.y - 60.0);
        farthest = farthest.max((dx * dx + dy * dy).sqrt());
    }
    assert!(farthest > 5.0 && farthest < 40.0, "{farthest}");
}

#[test]
fn panic_runs_a_member_along_the_path_on_the_panic_locomotor_set() {
    let mut setup = rig(vec![
        once("Make", make("U", "ZWalker", "teamGroup", "P1")),
        after(
            "Go",
            "U",
            vec![act("TEAM_PANIC", vec![text("teamGroup"), text("Run")])],
        ),
    ]);
    setup.teams = vec![team("Wilds", "teamGroup", true, &[])];
    setup.paths = vec![
        PathPoint {
            position: Vec2::new(30.0, 30.0),
            labels: [Some("Run".to_owned()), None, None],
            links: vec![1],
        },
        PathPoint {
            position: Vec2::new(200.0, 30.0),
            labels: [Some("Run".to_owned()), None, None],
            links: Vec::new(),
        },
    ];
    let mut sim = setup.start();
    run(&mut sim, 900);
    let ids = sim.rules().ids().unwrap();
    let unit = named(&sim, "U");
    assert!(sim.state().contains(ids.locomotor_override, unit));
    assert!((position_of(&sim, unit).x - 200.0).abs() < 10.0);
}
