use mint_movement::Vec2;
use mint_sim::{Entity, Simulation};

use super::p67_tests::{rig, team};
use super::w3a_tests::{act, after, named, number, once, run, text};
use super::PathPoint;
use crate::rules::RotwkRules;

const TEAM: &str = "teamGroup";

fn point(x: f32, y: f32, links: Vec<usize>) -> PathPoint {
    PathPoint {
        position: Vec2::new(x, y),
        labels: [Some("Run".to_owned()), None, None],
        links,
    }
}

fn spawn(unit: &str, waypoint: &str) -> Vec<mint_sage_map::ScriptAction> {
    vec![
        act(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                text(unit),
                text("ZSoldier"),
                text("teamWilds"),
                text(waypoint),
            ],
        ),
        act("UNIT_SET_TEAM", vec![text(unit), text(TEAM)]),
    ]
}

fn walking(verb: &str, as_team: i32, path: Vec<PathPoint>) -> Simulation<RotwkRules> {
    walking_with(
        vec![act(verb, vec![text(TEAM), text("Run"), number(as_team)])],
        path,
    )
}

fn walking_with(
    orders: Vec<mint_sage_map::ScriptAction>,
    path: Vec<PathPoint>,
) -> Simulation<RotwkRules> {
    walking_skirmish(orders, path, 0.0, false)
}

fn walking_skirmish(
    orders: Vec<mint_sage_map::ScriptAction>,
    path: Vec<PathPoint>,
    fudge: f32,
    ai_seat: bool,
) -> Simulation<RotwkRules> {
    let mut setup = rig(vec![
        once(
            "Make",
            [spawn("A", "SA"), spawn("B", "SB"), spawn("C", "SC")].concat(),
        ),
        after("Go", "C", orders),
    ]);
    setup.waypoints = vec![
        ("SA", 100.0, 100.0),
        ("SB", 300.0, 500.0),
        ("SC", 500.0, 100.0),
    ];
    setup.teams = vec![team("Wilds", TEAM, true, &[])];
    setup.paths = path;
    setup.fudge = fudge;
    setup.ai_seat = ai_seat;
    setup.start()
}

fn position(sim: &Simulation<RotwkRules>, unit: Entity) -> Vec2 {
    let ids = sim.rules().ids().unwrap();
    sim.state().get(ids.body, unit).unwrap().0.position
}

fn goal(sim: &Simulation<RotwkRules>, unit: Entity) -> Option<Vec2> {
    let ids = sim.rules().ids().unwrap();
    sim.state().get(ids.path, unit).map(|path| path.goal)
}

fn near(a: Vec2, b: Vec2, within: f32) -> bool {
    a.minus(b).length() <= within
}

fn two_legs() -> Vec<PathPoint> {
    vec![
        point(300.0, 350.0, vec![1]),
        point(700.0, 350.0, Vec::new()),
    ]
}

#[test]
fn a_team_walking_as_a_team_keeps_each_offset_from_the_centre_clamped_to_150() {
    let mut sim = walking("TEAM_FOLLOW_WAYPOINTS", 1, two_legs());
    run(&mut sim, 1500);
    let a = named(&sim, "A");
    let centre = Vec2::new(300.0, 700.0 / 3.0);
    let offset = Vec2::new(-200.0, -centre.y + 100.0);
    let clamped = offset.scale(150.0 / offset.length());
    assert!(
        near(
            position(&sim, a),
            Vec2::new(700.0, 350.0).plus(clamped),
            20.0
        ),
        "{:?}",
        position(&sim, a)
    );
}

#[test]
fn a_script_team_walk_leaves_the_members_speed_alone() {
    let mut sim = walking("TEAM_FOLLOW_WAYPOINTS", 1, two_legs());
    run(&mut sim, 60);
    let ids = sim.rules().ids().unwrap();
    let held = ["A", "B", "C"]
        .iter()
        .filter(|name| sim.state().contains(ids.formation_speed, named(&sim, name)))
        .count();
    assert_eq!(held, 0);
}

fn follow(verb: &str, as_team: i32, in_formation: i32) -> mint_sage_map::ScriptAction {
    act(
        verb,
        vec![
            text(TEAM),
            text("Run"),
            number(as_team),
            number(in_formation),
        ],
    )
}

#[test]
fn an_as_team_follow_ignores_the_formation_slot_the_last_group_move_left() {
    let mut sim = walking_with(
        vec![
            follow("TEAM_FOLLOW_WAYPOINTS", 0, 1),
            follow("TEAM_FOLLOW_WAYPOINTS", 1, 0),
        ],
        two_legs(),
    );
    run(&mut sim, 10);
    let a = named(&sim, "A");
    let offset = Vec2::new(-200.0, 100.0 - 700.0 / 3.0);
    let clamped = offset.scale(150.0 / offset.length());
    let aim = goal(&sim, a).expect("A has no route");
    assert!(
        near(aim, Vec2::new(300.0, 350.0).plus(clamped), 15.0),
        "{aim:?}"
    );
}

#[test]
fn an_in_formation_follow_walks_each_member_to_its_laid_out_slot() {
    let mut sim = walking_with(vec![follow("TEAM_FOLLOW_WAYPOINTS", 0, 1)], two_legs());
    run(&mut sim, 10);
    let ids = sim.rules().ids().unwrap();
    let mut slots = Vec::new();
    for name in ["A", "B", "C"] {
        let member = named(&sim, name);
        let slot = sim
            .state()
            .get(ids.formation_slot, member)
            .unwrap_or_else(|| panic!("{name} holds no slot"))
            .0;
        let aim = goal(&sim, member).expect("no route");
        assert!(
            near(aim, Vec2::new(300.0, 350.0).plus(slot), 15.0),
            "{name}: {aim:?} {slot:?}"
        );
        slots.push(slot);
    }
    assert!(
        slots.iter().any(|slot| slot.length() > 1.0),
        "no member was given a column slot: {slots:?}"
    );
}

#[test]
fn the_attack_follow_takes_as_team_offsets() {
    let mut sim = walking(
        "TEAM_ATTACK_MOVE_FOLLOW_WAYPOINTS",
        1,
        vec![
            point(300.0, 900.0, vec![1]),
            point(700.0, 900.0, Vec::new()),
        ],
    );
    run(&mut sim, 60);
    let a = named(&sim, "A");
    let aim = goal(&sim, a).expect("A has no route");
    let offset = Vec2::new(-200.0, 100.0 - 700.0 / 3.0);
    let clamped = offset.scale(150.0 / offset.length());
    assert!(
        near(aim, Vec2::new(300.0, 900.0).plus(clamped), 15.0),
        "{aim:?}"
    );
}

#[test]
fn a_team_is_close_enough_within_its_member_count_times_the_fudge() {
    let (centre, goal) = (Vec2::ZERO, Vec2::new(29.0, 0.0));
    assert!(super::walks::within_group_fudge(centre, goal, 3, 10.0));
    assert!(!super::walks::within_group_fudge(centre, goal, 2, 10.0));
}

#[test]
fn only_a_skirmish_ai_seat_takes_the_group_fudge_exit() {
    let aim_after = |seat: bool| {
        let mut sim = walking_skirmish(
            vec![act(
                "TEAM_FOLLOW_WAYPOINTS",
                vec![text(TEAM), text("Run"), number(1)],
            )],
            two_legs(),
            1000.0,
            seat,
        );
        run(&mut sim, 12);
        goal(&sim, named(&sim, "A")).expect("A has no route")
    };
    assert!(aim_after(true).x > 500.0, "the seat's team skipped leg 0");
    assert!(
        aim_after(false).x < 400.0,
        "a map player's team skipped leg 0"
    );
}

#[test]
fn an_attack_follow_leg_has_no_retries_and_is_non_final() {
    let mut sim = walking("TEAM_ATTACK_MOVE_FOLLOW_WAYPOINTS", 0, two_legs());
    run(&mut sim, 12);
    let ids = sim.rules().ids().unwrap();
    let order = *sim
        .state()
        .get(ids.attack_move_order, named(&sim, "A"))
        .expect("A holds no attack order");
    assert_eq!(
        (order.retries_left, order.follow, order.non_final),
        (0, true, true)
    );
}

#[test]
fn the_first_member_to_finish_a_leg_sends_the_whole_team_to_the_next_waypoint() {
    let mut sim = walking("TEAM_FOLLOW_WAYPOINTS", 1, two_legs());
    run(&mut sim, 60);
    let a = named(&sim, "A");
    let aim = goal(&sim, a).expect("A has no route");
    let centre_y = 700.0 / 3.0;
    let offset = Vec2::new(-200.0, 100.0 - centre_y);
    let clamped = offset.scale(150.0 / offset.length());
    assert!(
        near(aim, Vec2::new(700.0, 350.0).plus(clamped), 15.0),
        "{aim:?}"
    );
}

#[test]
fn an_exact_follower_takes_the_waypoint_chain_as_its_route() {
    let path = vec![
        point(300.0, 400.0, vec![1]),
        point(700.0, 100.0, Vec::new()),
    ];
    let mut sim = walking("TEAM_FOLLOW_WAYPOINTS_EXACT", 0, path);
    run(&mut sim, 40);
    let ids = sim.rules().ids().unwrap();
    let a = named(&sim, "A");
    let route = &sim.state().get(ids.path, a).expect("A has no route").route;
    let cell = sim.rules().passability.geometry.cell_size();
    assert!(route.unsearched);
    assert_eq!(route.points.get(1), Some(&Vec2::new(300.0, 400.0)));
    let last = route.points.last().expect("route has a last point");
    assert!((last.x - 700.0).abs() <= cell && (last.y - 100.0).abs() <= cell);
}
