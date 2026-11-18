use super::*;

const CLICK: Vec2 = Vec2::new(500.0, 150.0);

fn loose_units(at: &[Vec2]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let spawns = at
        .iter()
        .map(|p| spawn_at(p.x, p.y, KindOf::SELECTABLE, PLAYER))
        .collect();
    let simulation = Simulation::new(7, RotwkRules::new(passability, spawns)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, units)
}

fn order(simulation: &mut Simulation<RotwkRules>, units: Vec<Entity>, command: RotwkCommand) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
    simulation.submit(PLAYER, command).unwrap();
}

fn move_to(at: Vec2) -> RotwkCommand {
    RotwkCommand::MoveTo {
        x: at.x,
        y: at.y,
        queue: false,
    }
}

#[test]
fn each_member_of_a_player_move_heads_for_the_click() {
    let (mut simulation, ids, units) =
        loose_units(&[Vec2::new(100.0, 100.0), Vec2::new(100.0, 200.0)]);
    order(&mut simulation, units.clone(), move_to(CLICK));
    for _ in 0..500 {
        simulation.tick().unwrap();
    }
    for unit in units {
        let at = position(&simulation, ids, unit);
        assert!(at.minus(CLICK).length() < 25.0, "{unit:?} ended at {at:?}");
    }
}

#[test]
fn a_player_move_leaves_no_group_speed_cap() {
    let (mut simulation, ids, units) =
        loose_units(&[Vec2::new(100.0, 100.0), Vec2::new(100.0, 200.0)]);
    order(&mut simulation, units.clone(), move_to(CLICK));
    simulation.tick().unwrap();
    for unit in units {
        assert!(!simulation.state().contains(ids.formation_speed, unit));
    }
}

#[test]
fn the_first_selected_member_claims_the_click() {
    let (mut simulation, ids, units) =
        loose_units(&[Vec2::new(450.0, 150.0), Vec2::new(200.0, 150.0)]);
    let (near, far) = (units[0], units[1]);
    order(&mut simulation, vec![far, near], move_to(CLICK));
    for _ in 0..500 {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, far);
    assert!(
        at.minus(CLICK).length() < 6.0,
        "the far unit ended at {at:?}"
    );
}

#[test]
fn a_queued_leg_claims_its_point_when_it_starts() {
    let (mut simulation, ids, units) =
        loose_units(&[Vec2::new(100.0, 100.0), Vec2::new(450.0, 150.0)]);
    let (late, early) = (units[0], units[1]);
    order(
        &mut simulation,
        vec![late],
        move_to(Vec2::new(100.0, 300.0)),
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: CLICK.x,
                y: CLICK.y,
                queue: true,
            },
        )
        .unwrap();
    order(&mut simulation, vec![early], move_to(CLICK));
    for _ in 0..1500 {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, late);
    assert!(
        at.minus(CLICK).length() > 10.0,
        "the queued unit ended at {at:?}"
    );
}

fn goal_cell_offsets(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    units: &[Entity],
) -> Vec<(i32, i32)> {
    units
        .iter()
        .map(|unit| {
            let goal = simulation.state().get(ids.path, *unit).unwrap().goal;
            #[allow(clippy::cast_possible_truncation)]
            let offset = |g: f32, c: f32| ((g - c) / 10.0).round() as i32;
            (offset(goal.x, CLICK.x + 5.0), offset(goal.y, CLICK.y + 5.0))
        })
        .collect()
}

#[test]
fn members_of_a_player_move_take_cells_in_the_retail_spiral_order() {
    let (mut simulation, ids, units) = loose_units(&[
        Vec2::new(100.0, 100.0),
        Vec2::new(100.0, 120.0),
        Vec2::new(100.0, 140.0),
        Vec2::new(100.0, 160.0),
        Vec2::new(100.0, 180.0),
    ]);
    order(&mut simulation, units.clone(), move_to(CLICK));
    simulation.tick().unwrap();
    assert_eq!(
        goal_cell_offsets(&simulation, ids, &units),
        [(0, 0), (1, 0), (0, 1), (-1, 0), (1, 1)]
    );
}

#[test]
fn a_body_standing_on_the_click_cell_sends_the_first_member_to_a_neighbour() {
    let (mut simulation, ids, units) = loose_units(&[
        Vec2::new(100.0, 100.0),
        Vec2::new(CLICK.x + 5.0, CLICK.y + 5.0),
    ]);
    order(&mut simulation, vec![units[0]], move_to(CLICK));
    simulation.tick().unwrap();
    assert_ne!(goal_cell_offsets(&simulation, ids, &units[..1]), [(0, 0)]);
}

fn alt_move_to(at: Vec2) -> RotwkCommand {
    RotwkCommand::MoveTo {
        x: at.x,
        y: at.y,
        queue: true,
    }
}

#[test]
fn an_alt_click_on_a_member_running_a_plain_order_loops_through_it() {
    let (mut simulation, ids, units) = loose_units(&[Vec2::new(100.0, 100.0)]);
    order(
        &mut simulation,
        units.clone(),
        move_to(Vec2::new(300.0, 100.0)),
    );
    simulation.tick().unwrap();
    simulation
        .submit(PLAYER, alt_move_to(Vec2::new(105.0, 100.0)))
        .unwrap();
    simulation.tick().unwrap();
    let queue = simulation.state().get(ids.order_queue, units[0]).unwrap();
    assert_eq!(queue.loop_start, Some(0));
}

#[test]
fn a_laps_copy_of_the_start_order_has_a_fresh_id() {
    let (mut simulation, ids, units) = loose_units(&[Vec2::new(100.0, 100.0)]);
    order(
        &mut simulation,
        units.clone(),
        move_to(Vec2::new(200.0, 100.0)),
    );
    simulation.tick().unwrap();
    simulation
        .submit(PLAYER, alt_move_to(Vec2::new(105.0, 100.0)))
        .unwrap();
    let first = simulation
        .state()
        .get(ids.order_queue, units[0])
        .unwrap()
        .current
        .as_ref()
        .and_then(|leg| leg.tag.as_ref())
        .map(|tag| tag.id)
        .unwrap();
    let mut copy = None;
    for _ in 0..600 {
        simulation.tick().unwrap();
        copy = simulation
            .state()
            .get(ids.order_queue, units[0])
            .and_then(|queue| {
                queue
                    .queued
                    .iter()
                    .filter_map(|leg| leg.tag.clone())
                    .find(|tag| tag.copy_of == Some(first))
            });
        if copy.is_some() {
            break;
        }
    }
    assert_ne!(copy.expect("the start order was never copied").id, first);
}

#[test]
fn a_laggard_finishing_the_original_gets_the_leaders_copy_id() {
    let (mut simulation, ids, units) =
        loose_units(&[Vec2::new(100.0, 100.0), Vec2::new(100.0, 1200.0)]);
    order(
        &mut simulation,
        units.clone(),
        move_to(Vec2::new(300.0, 100.0)),
    );
    simulation.tick().unwrap();
    simulation
        .submit(PLAYER, alt_move_to(Vec2::new(105.0, 100.0)))
        .unwrap();
    let first = simulation
        .state()
        .get(ids.order_queue, units[0])
        .unwrap()
        .current
        .as_ref()
        .and_then(|leg| leg.tag.as_ref())
        .map(|tag| tag.id)
        .unwrap();
    let copy_in = |simulation: &Simulation<RotwkRules>, unit: Entity| {
        simulation
            .state()
            .get(ids.order_queue, unit)
            .and_then(|queue| {
                queue
                    .queued
                    .iter()
                    .filter_map(|leg| leg.tag.clone())
                    .find(|tag| tag.copy_of == Some(first))
            })
    };
    let mut leaders = None;
    let mut laggards = None;
    for _ in 0..4000 {
        simulation.tick().unwrap();
        if leaders.is_none() {
            leaders = copy_in(&simulation, units[0]).map(|tag| tag.id);
        }
        if laggards.is_none() {
            laggards = copy_in(&simulation, units[1]).map(|tag| tag.id);
        }
        if laggards.is_some() {
            break;
        }
    }
    assert_eq!(laggards, leaders);
    assert!(leaders.is_some());
}
