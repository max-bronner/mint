use super::*;

fn bordered(samples: u32, border: u32) -> Passability {
    let mut map = flat_map(samples, samples);
    map.border_size = border;
    let playable = i32::try_from(samples - 1 - 2 * border).unwrap();
    map.boundaries = vec![mint_sage_map::Boundary {
        x: playable,
        y: playable,
    }];
    Passability::builder(&map, 5.0 / 128.0)
        .playable_border(true)
        .build()
        .unwrap()
}

fn order(simulation: &mut Simulation<RotwkRules>, entity: Entity, to: Vec2, queue: bool) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: to.x,
                y: to.y,
                queue,
            },
        )
        .unwrap();
}

fn walker_at(passability: Passability, at: Vec2) -> (Simulation<RotwkRules>, Entity) {
    let rules = RotwkRules::new(
        passability,
        vec![spawn_at(at.x, at.y, KindOf::SELECTABLE, PLAYER)],
    );
    let simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entity = simulation.state().store(ids.body).unwrap().entities()[0];
    (simulation, entity)
}

#[test]
fn a_unit_standing_off_the_map_walks_onto_it() {
    let (mut simulation, unit) = walker_at(bordered(61, 10), Vec2::new(200.0, -80.0));
    let ids = simulation.rules().ids().unwrap();
    order(&mut simulation, unit, Vec2::new(200.0, 200.0), false);
    for _ in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, unit);
    assert!(at.y > 150.0, "still off the map at {at:?}");
}

#[test]
fn a_unit_off_the_map_walks_to_a_goal_off_the_map() {
    let rules = RotwkRules::new(
        bordered(61, 10),
        vec![spawn_at(200.0, -130.0, KindOf::SELECTABLE, OTHER)],
    )
    .with_players(1);
    let mut simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation.state().store(ids.body).unwrap().entities()[0];
    let goal = Vec2::new(320.0, -130.0);
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![unit],
        },
        RotwkCommand::MoveTo {
            x: goal.x,
            y: goal.y,
            queue: false,
        },
    ] {
        simulation.submit(OTHER, command).unwrap();
    }
    for _ in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, unit);
    assert!(at.minus(goal).length() < 15.0, "stopped at {at:?}");
}

#[test]
fn a_unit_off_the_grid_walks_onto_the_map() {
    let (mut simulation, unit) = walker_at(bordered(61, 10), Vec2::new(200.0, -130.0));
    let ids = simulation.rules().ids().unwrap();
    order(&mut simulation, unit, Vec2::new(200.0, 200.0), false);
    for _ in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, unit);
    assert!(at.y > 150.0, "still off the map at {at:?}");
}

#[test]
fn a_leg_appended_behind_an_unqueued_move_runs_after_it() {
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let (mut simulation, unit) = walker_at(passability, Vec2::new(100.0, 100.0));
    let ids = simulation.rules().ids().unwrap();
    order(&mut simulation, unit, Vec2::new(300.0, 100.0), false);
    simulation.tick().unwrap();
    order(&mut simulation, unit, Vec2::new(300.0, 400.0), true);
    for _ in 0..(30 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, unit);
    assert!(at.y > 350.0, "stopped short of the appended leg at {at:?}");
}

fn duel_across_the_edge(shooter_y: f32, target_y: f32) -> f32 {
    let (combat, bow, armour) = combat_data();
    let spawns = vec![
        armed_spawn(200.0, shooter_y, PLAYER, Some(bow), Some(armour), 100.0),
        armed_spawn(200.0, target_y, OTHER, None, Some(armour), 100.0),
    ];
    let rules = RotwkRules::new(bordered(61, 10), spawns).with_combat(combat);
    let mut simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let target = simulation.state().store(ids.body).unwrap().entities()[1];
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    health_of(&simulation, ids, target).unwrap_or(0.0)
}

#[test]
fn an_idle_unit_off_the_map_acquires_nothing() {
    assert!((duel_across_the_edge(-30.0, 30.0) - 100.0).abs() < 1e-3);
}

#[test]
fn an_idle_unit_on_the_map_leaves_a_foe_beyond_the_edge() {
    assert!(
        duel_across_the_edge(30.0, 62.0) < 100.0,
        "the on-map control was not shot"
    );
    assert!((duel_across_the_edge(30.0, -2.0) - 100.0).abs() < 1e-3);
}
