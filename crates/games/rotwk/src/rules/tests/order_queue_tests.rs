use super::*;

fn single_horde_rules(
    passability: Passability,
    enemies: Vec<Spawn>,
    weapon: mint_sage_combat::WeaponId,
    armour: mint_sage_combat::ArmorId,
) -> RotwkRules {
    let formation = square_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new(200.0, 200.0),
        facing: Facing::EAST,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(weapon),
            armour: Some(armour),
            vision: Some(400.0),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    RotwkRules::new(passability, enemies).with_hordes(vec![formation], vec![spawn])
}

#[test]
fn an_appended_order_starts_only_if_the_queue_was_empty() {
    let (combat, bow, armour) = combat_data();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = single_horde_rules(passability, Vec::new(), bow, armour).with_combat(combat);
    let mut simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();

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
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 200.0,
                queue: true,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(simulation.state().contains(ids.path, horde));
    let queue_after_first = simulation.state().get(ids.order_queue, horde).cloned();
    assert!(queue_after_first.as_ref().unwrap().queued.is_empty());

    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 600.0,
                queue: true,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let queue = simulation
        .state()
        .get(ids.order_queue, horde)
        .cloned()
        .unwrap();
    assert_eq!(queue.current.unwrap().point, Vec2::new(605.0, 205.0));
    assert_eq!(queue.queued.len(), 1);
    assert_eq!(queue.queued[0].point, Vec2::new(600.0, 600.0));
}

#[test]
fn a_replacing_order_clears_the_waypoint_queue() {
    let (combat, bow, armour) = combat_data();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = single_horde_rules(passability, Vec::new(), bow, armour).with_combat(combat);
    let mut simulation = Simulation::new(12, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();

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
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 200.0,
                queue: true,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 600.0,
                queue: true,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation
            .state()
            .get(ids.order_queue, horde)
            .unwrap()
            .queued
            .len(),
        1
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 50.0,
                y: 50.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(simulation
        .state()
        .get(ids.order_queue, horde)
        .is_none_or(|queue| queue.queued.is_empty() && queue.loop_start.is_none()));
}

#[test]
fn a_leg_ends_within_20_of_its_point_and_the_next_leg_starts() {
    let (combat, bow, armour) = combat_data();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = single_horde_rules(passability, Vec::new(), bow, armour).with_combat(combat);
    let mut simulation = Simulation::new(13, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();

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
            RotwkCommand::MoveTo {
                x: 215.0,
                y: 200.0,
                queue: true,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 200.0,
                queue: true,
            },
        )
        .unwrap();
    let mut became_current = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        if let Some(queue) = simulation.state().get(ids.order_queue, horde) {
            if queue.current.as_ref().map(|leg| leg.point) == Some(Vec2::new(605.0, 205.0)) {
                became_current = true;
                break;
            }
        } else {
            break;
        }
    }
    assert!(
        became_current,
        "the second leg must become current once the first is within 20.0 of its point"
    );
}

#[test]
fn a_plain_leg_ignores_an_enemy_an_attack_leg_engages() {
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let (combat, bow, armour) = combat_data();
    let enemy = armed_spawn(450.0, 200.0, OTHER, None, Some(armour), 100.0);
    let rules = single_horde_rules(passability, vec![enemy], bow, armour).with_combat(combat);
    let mut simulation = Simulation::new(14, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();

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
            RotwkCommand::MoveTo {
                x: 300.0,
                y: 200.0,
                queue: true,
            },
        )
        .unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation
            .state()
            .get(ids.horde_target, horde)
            .is_none_or(|hordetarget| hordetarget.target.is_none()),
        "a plain leg must not auto-acquire an enemy within its own weapon range"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackMove {
                x: 700.0,
                y: 200.0,
                queue: true,
            },
        )
        .unwrap();
    let mut engaged = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.horde_target, horde)
            .is_some_and(|hordetarget| {
                hordetarget.target == Some(enemy_entity(&simulation, ids, OTHER))
            })
        {
            engaged = true;
            break;
        }
    }
    assert!(engaged, "the attack-move leg must auto-acquire the enemy");
}

fn enemy_entity(simulation: &Simulation<RotwkRules>, ids: Ids, owner: PlayerId) -> Entity {
    simulation
        .state()
        .store(ids.owner)
        .map(|store| {
            store
                .iter()
                .find(|(_, candidate)| candidate.0 == owner)
                .map(|(entity, _)| entity)
        })
        .unwrap()
        .unwrap()
}

fn returns_to_b(third: Vec2, ticks: u32) -> u32 {
    let (combat, bow, armour) = combat_data();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = single_horde_rules(passability, Vec::new(), bow, armour).with_combat(combat);
    let mut simulation = Simulation::new(13, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    for at in [Vec2::new(400.0, 200.0), Vec2::new(400.0, 400.0), third] {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: at.x,
                    y: at.y,
                    queue: true,
                },
            )
            .unwrap();
    }
    let b = Vec2::new(400.0, 400.0);
    let (mut visits, mut at_b) = (0, false);
    for _ in 0..ticks {
        simulation.tick().unwrap();
        let near = position(&simulation, ids, horde).minus(b).length() < 60.0;
        if near && !at_b {
            visits += 1;
        }
        at_b = near;
    }
    visits
}

#[test]
fn an_alt_click_near_an_earlier_one_loops_the_chain() {
    assert!(returns_to_b(Vec2::new(405.0, 205.0), 3000) >= 2);
}

#[test]
fn an_alt_click_20_away_does_not_loop() {
    assert_eq!(returns_to_b(Vec2::new(400.0, 220.0), 3000), 1);
}

#[test]
fn the_queue_strip_runs_through_every_queued_leg_and_marks_the_loop() {
    let (combat, bow, armour) = combat_data();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = single_horde_rules(passability, Vec::new(), bow, armour).with_combat(combat);
    let mut simulation = Simulation::new(13, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    for at in [
        Vec2::new(400.0, 200.0),
        Vec2::new(400.0, 400.0),
        Vec2::new(405.0, 205.0),
    ] {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: at.x,
                    y: at.y,
                    queue: true,
                },
            )
            .unwrap();
    }
    simulation.tick().unwrap();
    let strip = simulation
        .rules()
        .order_queue_strip(simulation.state(), horde)
        .expect("a queue with legs behind the running one has a strip");
    assert_eq!(strip.points.len(), 4, "the object, then three legs");
    assert_eq!(strip.points[2], Vec2::new(400.0, 400.0));
    assert!(
        strip.loop_from.is_some(),
        "the third click closed the chain"
    );
}
