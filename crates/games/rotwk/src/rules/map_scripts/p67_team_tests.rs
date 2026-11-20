use mint_movement::Vec2;
use mint_sage_map::{ScriptAction, Value};
use mint_sim::{Entity, Simulation};

use super::p67_tests::{rig, team, P0, P1, SIDE};
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
fn wander_walks_a_member_along_the_path_to_its_end() {
    let mut setup = rig(vec![
        once("Make", make("U", "ZWalker", "teamGroup", "P1")),
        after(
            "Go",
            "U",
            vec![act("TEAM_WANDER", vec![text("teamGroup"), text("Roam")])],
        ),
    ]);
    setup.teams = vec![team("Wilds", "teamGroup", true, &[])];
    setup.paths = vec![
        PathPoint {
            position: Vec2::new(30.0, 30.0),
            labels: [Some("Roam".to_owned()), None, None],
            links: vec![1],
        },
        PathPoint {
            position: Vec2::new(200.0, 30.0),
            labels: [Some("Roam".to_owned()), None, None],
            links: Vec::new(),
        },
    ];
    let mut sim = setup.start();
    run(&mut sim, 12);
    let unit = named(&sim, "U");
    let mut farthest = 0.0_f32;
    for _ in 0..900 {
        run(&mut sim, 1);
        farthest = farthest.max(position_of(&sim, unit).x);
    }
    assert!(farthest > 190.0, "{farthest}");
}

fn wander_rig(links_back: bool) -> Simulation<RotwkRules> {
    let mut setup = rig(vec![
        once("Make", make("U", "ZWalker", "teamGroup", "P1")),
        after(
            "Go",
            "U",
            vec![act("TEAM_WANDER", vec![text("teamGroup"), text("Roam")])],
        ),
    ]);
    setup.teams = vec![team("Wilds", "teamGroup", true, &[])];
    let label = || [Some("Roam".to_owned()), None, None];
    setup.paths = vec![
        PathPoint {
            position: Vec2::new(30.0, 30.0),
            labels: label(),
            links: vec![1],
        },
        PathPoint {
            position: Vec2::new(120.0, 30.0),
            labels: label(),
            links: if links_back { vec![0] } else { Vec::new() },
        },
    ];
    let mut sim = setup.start();
    run(&mut sim, 12);
    sim
}

#[test]
fn a_wander_that_ends_records_the_finished_walk() {
    let mut sim = wander_rig(false);
    run(&mut sim, 900);
    let unit = named(&sim, "U");
    assert!(state_of(&sim).rt.walks.finished(unit, "Roam"));
}

#[test]
fn a_looped_wander_path_never_ends() {
    let mut sim = wander_rig(true);
    run(&mut sim, 1500);
    let unit = named(&sim, "U");
    assert!(!state_of(&sim).rt.walks.finished(unit, "Roam"));
    assert!(position_of(&sim, unit).x > 100.0 || position_of(&sim, unit).x < 60.0);
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

fn panic_rig(far_end: f32) -> Simulation<RotwkRules> {
    let mut setup = rig(vec![
        once("Make", make("U", "ZWalker", "teamGroup", "P1")),
        after(
            "Go",
            "U",
            vec![act("TEAM_PANIC", vec![text("teamGroup"), text("Run")])],
        ),
    ]);
    setup.teams = vec![team("Wilds", "teamGroup", true, &[])];
    let label = || [Some("Run".to_owned()), None, None];
    setup.paths = vec![
        PathPoint {
            position: Vec2::new(30.0, 30.0),
            labels: label(),
            links: vec![1],
        },
        PathPoint {
            position: Vec2::new(far_end, 30.0),
            labels: label(),
            links: Vec::new(),
        },
    ];
    setup.start()
}

fn panicking(sim: &Simulation<RotwkRules>, unit: Entity) -> bool {
    let wiring = sim.rules().modules.as_ref().unwrap().state;
    sim.state().get(wiring, unit).is_some_and(|state| {
        state
            .model_conditions
            .iter()
            .any(|name| name.to_string() == "PANICKING")
    })
}

#[test]
fn a_panic_at_its_last_waypoint_is_idle_and_not_panicking() {
    let mut sim = panic_rig(200.0);
    run(&mut sim, 40);
    let unit = named(&sim, "U");
    assert!(panicking(&sim, unit));
    run(&mut sim, 900);
    let ids = sim.rules().ids().unwrap();
    assert!(!panicking(&sim, unit));
    assert!(!state_of(&sim).rt.wanderers.contains(unit));
    assert!(state_of(&sim).rt.walks.finished(unit, "Run"));
    assert!(sim.state().contains(ids.locomotor_override, unit));
}

#[test]
fn a_panic_walk_that_cannot_arrive_keeps_the_member_in_the_state() {
    let mut sim = panic_rig(50_000.0);
    run(&mut sim, 1500);
    let unit = named(&sim, "U");
    assert!(panicking(&sim, unit));
    assert!(state_of(&sim).rt.wanderers.contains(unit));
    assert!(!state_of(&sim).rt.walks.finished(unit, "Run"));
}

fn panic_rig_with(
    placed: Vec<(&'static str, f32, f32, mint_sim::PlayerId)>,
    far: f32,
) -> Simulation<RotwkRules> {
    let mut setup = rig(vec![
        once("Make", make("U", "ZWalker", "teamGroup", "P1")),
        after(
            "Go",
            "U",
            vec![act("TEAM_PANIC", vec![text("teamGroup"), text("Run")])],
        ),
    ]);
    setup.placed = placed;
    setup.teams = vec![team("Wilds", "teamGroup", true, &[])];
    let label = || [Some("Run".to_owned()), None, None];
    setup.paths = vec![
        PathPoint {
            position: Vec2::new(30.0, 30.0),
            labels: label(),
            links: vec![1],
        },
        PathPoint {
            position: Vec2::new(far, 30.0),
            labels: label(),
            links: Vec::new(),
        },
    ];
    setup.start()
}

fn goal_of(sim: &Simulation<RotwkRules>, unit: Entity) -> Option<Vec2> {
    let ids = sim.rules().ids().unwrap();
    sim.state().get(ids.path, unit).map(|path| path.goal)
}

#[test]
fn a_panic_walk_with_an_onward_link_advances_under_twice_its_speed() {
    let mut sim = panic_rig(400.0);
    let ids = sim.rules().ids().unwrap();
    let mut seen = None;
    for _ in 0..200 {
        run(&mut sim, 1);
        let unit = sim.state().store(ids.script_name).and_then(|store| {
            store
                .iter()
                .find(|(_, tag)| tag.0 == "U")
                .map(|(entity, _)| entity)
        });
        let Some(unit) = unit else { continue };
        if goal_of(&sim, unit).is_some_and(|goal| goal.x > 100.0) {
            seen = Some(
                position_of(&sim, unit)
                    .minus(Vec2::new(30.0, 30.0))
                    .length(),
            );
            break;
        }
    }
    let left = seen.expect("the walk advanced to the second waypoint");
    assert!(left > 5.0, "advanced {left} from the first waypoint");
}

#[test]
fn a_panic_walk_arrives_at_the_end_of_an_adjusted_route() {
    let mut sim = panic_rig_with(vec![("ZSoldier", 200.0, 30.0, P1)], 200.0);
    run(&mut sim, 900);
    let unit = named(&sim, "U");
    assert!(
        position_of(&sim, unit)
            .minus(Vec2::new(200.0, 30.0))
            .length()
            > 1.0
    );
    assert!(state_of(&sim).rt.walks.finished(unit, "Run"));
}

fn repulsor_rig(path_wander: bool) -> Simulation<RotwkRules> {
    let mut setup = rig(vec![
        once("Make", make("U", "ZTownie", "teamGroup", "P1")),
        once("MakeScarer", make("R", "ZVictim", "teamGroup", "P2")),
        after(
            "Go",
            "U",
            if path_wander {
                vec![act("TEAM_WANDER", vec![text("teamGroup"), text("Roam")])]
            } else {
                Vec::new()
            },
        ),
        after(
            "Mark",
            "R",
            vec![act("NAMED_SET_REPULSOR", vec![text("R"), number(1)])],
        ),
    ]);
    setup.teams = vec![team("Wilds", "teamGroup", true, &[])];
    let label = || [Some("Roam".to_owned()), None, None];
    setup.paths = vec![
        PathPoint {
            position: Vec2::new(30.0, 30.0),
            labels: label(),
            links: vec![1],
        },
        PathPoint {
            position: Vec2::new(120.0, 30.0),
            labels: label(),
            links: vec![0],
        },
    ];
    let mut sim = setup.start();
    run(&mut sim, 40);
    sim
}

#[test]
fn named_set_repulsor_marks_the_object_a_repulsor() {
    let sim = repulsor_rig(false);
    let scarer = named(&sim, "R");
    assert!(sim.rules().has_status_bit(sim.state(), scarer, "REPULSOR"));
}

#[test]
fn a_repulsed_wanderer_leaves_its_path_for_the_flee_state() {
    let sim = repulsor_rig(true);
    let ids = sim.rules().ids().unwrap();
    let unit = named(&sim, "U");
    assert!(sim.state().contains(ids.repulsed, unit));
    assert!(!state_of(&sim).rt.wanderers.contains(unit));
}
