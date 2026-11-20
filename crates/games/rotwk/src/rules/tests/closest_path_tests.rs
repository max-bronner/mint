use mint_sage_passability::{Footprint, FootprintRecord};

use super::*;

const POCKET: Vec2 = Vec2::new(400.0, 300.0);

fn wall(major: f32, minor: f32) -> Vec<FootprintRecord> {
    vec![FootprintRecord {
        offset: Vec2::ZERO,
        footprint: Footprint::Box { major, minor },
    }]
}

fn sealed_pocket(from: Vec2) -> (Simulation<RotwkRules>, Ids, Entity) {
    sealed_pocket_with(from, None)
}

fn sealed_pocket_with(from: Vec2, standing: Option<Vec2>) -> (Simulation<RotwkRules>, Ids, Entity) {
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let sides = [
        (Vec2::new(400.0, 350.0), 60.0, 10.0),
        (Vec2::new(400.0, 250.0), 60.0, 10.0),
        (Vec2::new(350.0, 300.0), 10.0, 60.0),
        (Vec2::new(450.0, 300.0), 10.0, 60.0),
    ];
    let mut spawns = vec![spawn_at(from.x, from.y, KindOf::SELECTABLE, PLAYER)];
    let mut footprints = Vec::new();
    for (at, major, minor) in sides {
        footprints.push((spawns.len(), wall(major, minor)));
        spawns.push(spawn_at(at.x, at.y, KindOf::STRUCTURE, PLAYER));
    }
    if let Some(at) = standing {
        spawns.push(spawn_at(at.x, at.y, KindOf::SELECTABLE, PLAYER));
    }
    let rules = RotwkRules::new(passability, spawns).with_structure_footprints(footprints);
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    (simulation, ids, unit)
}

fn inside_the_pocket(at: Vec2) -> bool {
    (360.0..=440.0).contains(&at.x) && (260.0..=340.0).contains(&at.y)
}

fn order(simulation: &mut Simulation<RotwkRules>, unit: Entity, at: Vec2) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: at.x,
                y: at.y,
                queue: false,
            },
        )
        .unwrap();
}

#[test]
fn an_order_into_a_sealed_pocket_walks_to_the_nearest_reachable_point_and_stops() {
    let (mut simulation, ids, unit) = sealed_pocket(Vec2::new(200.0, 300.0));
    order(&mut simulation, unit, POCKET);
    for _ in 0..400 {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, unit);
    assert!(
        at.minus(POCKET).length() < 80.0,
        "walked up against the pocket's wall, stopped at {at:?}"
    );
    assert!(
        !inside_the_pocket(at),
        "and stayed outside the sealed ground: {at:?}"
    );
    assert!(
        !simulation.state().contains(ids.path, unit),
        "the order is finished, not still being searched for"
    );
}

#[test]
fn an_unreachable_order_replaces_the_route_the_unit_was_walking() {
    let (mut simulation, ids, unit) = sealed_pocket(Vec2::new(200.0, 300.0));
    order(&mut simulation, unit, Vec2::new(200.0, 550.0));
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let underway = position(&simulation, ids, unit);
    assert!(
        underway.y > 320.0,
        "the first order is underway: {underway:?}"
    );
    order(&mut simulation, unit, POCKET);
    for _ in 0..400 {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, unit);
    assert!(
        at.minus(POCKET).length() < 80.0,
        "the new order took over from the old route, ended at {at:?}"
    );
}

#[test]
fn an_attack_move_into_a_sealed_pocket_walks_to_the_nearest_reachable_point() {
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let sides = [
        (Vec2::new(400.0, 410.0), 120.0, 10.0),
        (Vec2::new(400.0, 190.0), 120.0, 10.0),
        (Vec2::new(290.0, 300.0), 10.0, 120.0),
        (Vec2::new(510.0, 300.0), 10.0, 120.0),
    ];
    let mut spawns = Vec::new();
    let mut footprints = Vec::new();
    for (at, major, minor) in sides {
        footprints.push((spawns.len(), wall(major, minor)));
        spawns.push(spawn_at(at.x, at.y, KindOf::STRUCTURE, PLAYER));
    }
    let formation = square_formation();
    let horde = HordeSpawn {
        formation: 0,
        origin: Vec2::new(120.0, 300.0),
        facing: Facing::EAST,
        member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER),
        count: u32::try_from(formation.len()).unwrap(),
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(passability, spawns)
        .with_structure_footprints(footprints)
        .with_hordes(vec![formation], vec![horde]);
    let mut simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackMove {
                x: POCKET.x,
                y: POCKET.y,
                queue: false,
            },
        )
        .unwrap();
    let horde = horde_entity(&simulation, ids).unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation
            .state()
            .get(ids.attack_move_order, horde)
            .is_some_and(|order| order.retries_left > 0),
        "the order is held at the route's end for its retries, not ended on arriving"
    );
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, horde);
    assert!(
        at.x > 200.0,
        "walked up against the pocket's wall, stopped at {at:?}"
    );
    assert!(
        !simulation.state().contains(ids.attack_move_order, horde),
        "the order ends after its fifth retry"
    );
}

#[test]
fn a_move_onto_a_wide_cliff_walks_to_its_edge() {
    let (width, height) = (64u32, 64u32);
    let mut samples = vec![0u16; (width * height) as usize];
    for y in 20..44u32 {
        for x in 30..60u32 {
            if (x + y) % 2 == 0 {
                if let Some(sample) = samples.get_mut((y * width + x) as usize) {
                    *sample = 400;
                }
            }
        }
    }
    let map = HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples,
    };
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(
        passability,
        vec![spawn_at(100.0, 320.0, KindOf::SELECTABLE, PLAYER)],
    );
    let mut simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    order(&mut simulation, unit, Vec2::new(450.0, 320.0));
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, unit);
    assert!(
        at.minus(Vec2::new(450.0, 320.0)).length() < 160.0,
        "walked up to the cliffs' edge nearest the click, stopped at {at:?}"
    );
}

#[test]
fn the_closest_cell_is_not_one_another_unit_has_claimed() {
    let far = Vec2::new(500.0, 330.0);
    let route_end = |claim: Option<Vec2>| {
        let (mut simulation, ids, unit) = sealed_pocket_with(Vec2::new(200.0, 300.0), Some(far));
        let other = simulation
            .state()
            .store(ids.body)
            .unwrap()
            .iter()
            .find(|(_, body)| body.0.position == far)
            .map(|(entity, _)| entity)
            .unwrap();
        if let Some(at) = claim {
            order(&mut simulation, other, at);
            simulation.tick().unwrap();
        }
        order(&mut simulation, unit, POCKET);
        simulation.tick().unwrap();
        simulation.state().get(ids.path, unit).unwrap().goal
    };
    let alone = route_end(None);
    let crowded = route_end(Some(alone));
    assert!(
        crowded.minus(alone).length() >= 8.0,
        "the route still ends on the claimed point: {crowded:?} vs {alone:?}"
    );
}

#[test]
fn a_goal_inside_an_obstacle_is_stepped_back_toward_the_mover() {
    let map = flat_map(64, 16);
    let block = mint_sage_passability::Obstacle {
        id: 0,
        centre: Vec2::new(100.0, 50.0),
        angle: 0.0,
        records: wall(40.0, 100.0),
    };
    let passability = Passability::builder(&map, 5.0 / 128.0)
        .obstacles(std::slice::from_ref(&block))
        .build()
        .unwrap();
    let start = passability.cell_at(Vec2::new(400.0, 50.0)).unwrap();
    let goal = passability.cell_at(Vec2::new(100.0, 50.0)).unwrap();
    let rules = RotwkRules::new(passability, Vec::new());
    let request = PathRequest {
        start,
        goal,
        mover: GROUND,
        goal_id: GoalId(0),
        origin: None,
        corner_radius: 0.0,
        departure: None,
        approach: None,
        prune: false,
        final_goal: true,
    };
    let aimed = rules.stepped_back(&request).goal;
    assert_ne!(aimed, goal, "the goal is inside the block");
    let at = rules.passability.geometry.centre_of(aimed);
    assert!(
        (140.0..170.0).contains(&at.x),
        "stepped back out of the block, not past the mover: {at:?}"
    );
}

#[test]
fn an_attack_move_goal_object_that_rose_past_the_limit_regoals() {
    let at = Vec2::new(10.0, 10.0);
    assert!(crate::rules::attack::regoal_due((at, 10.0), (at, 0.0)));
    assert!(!crate::rules::attack::regoal_due((at, 4.0), (at, 0.0)));
}
