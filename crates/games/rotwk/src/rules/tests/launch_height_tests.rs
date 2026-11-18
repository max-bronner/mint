use super::*;

fn first_flight_ends(shooter_up: Option<f32>, victim_up: Option<f32>) -> (f32, f32) {
    let (combat, bow, armour) = combat_data();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let shooter = Spawn {
        ground_offset: shooter_up,
        ..armed_spawn(200.0, 200.0, PLAYER, Some(bow), Some(armour), 100.0)
    };
    let victim = Spawn {
        ground_offset: victim_up,
        ..armed_spawn(350.0, 200.0, OTHER, None, Some(armour), 100.0)
    };
    let mut simulation = Simulation::new(
        11,
        RotwkRules::new(passability, vec![shooter, victim]).with_combat(combat),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0]],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: units[1] })
        .unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
        let first = simulation
            .state()
            .store(ids.flight)
            .and_then(|store| store.iter().next().map(|(_, flight)| *flight));
        if let Some(flight) = first {
            let points = flight.flight.control_points();
            return (points[0].z, points[3].z);
        }
    }
    panic!("no shot was fired");
}

#[test]
fn a_shot_launches_from_above_a_raised_shooter_and_lands_on_a_raised_victim() {
    let (from, to) = first_flight_ends(None, None);
    let (raised_from, _) = first_flight_ends(Some(51.0), None);
    assert!(
        (raised_from - from - 51.0).abs() < 1e-3,
        "a shooter 51 above the ground launched at {raised_from}, on the ground at {from}"
    );
    let (_, raised_to) = first_flight_ends(None, Some(40.0));
    assert!(
        (raised_to - to - 40.0).abs() < 1e-3,
        "a victim 40 above the ground was aimed at {raised_to}, on the ground at {to}"
    );
}

#[test]
fn shots_leave_by_randomly_drawn_launch_bones_turned_by_the_shooters_facing() {
    let (combat, bow, armour) = combat_data();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let shooter = Spawn {
        kind: Some(crate::unit::Kind(0)),
        ..armed_spawn(200.0, 200.0, PLAYER, Some(bow), Some(armour), 100.0)
    };
    let victim = armed_spawn(200.0, 350.0, OTHER, None, Some(armour), 1.0e6);
    let bone = |x: f32, y: f32| crate::perches::Perch {
        position: [x, y, 20.0],
        facing: Facing::EAST,
    };
    let primary = vec![bone(10.0, 0.0), bone(10.0, 5.0)];
    let mut simulation = Simulation::new(
        11,
        RotwkRules::new(passability, vec![shooter, victim])
            .with_combat(combat)
            .with_launch_bones(vec![vec![primary]]),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0]],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: units[1] })
        .unwrap();
    let mut seen = Vec::new();
    let mut starts = Vec::new();
    for _ in 0..3000 {
        simulation.tick().unwrap();
        let flights: Vec<(Entity, [f32; 3])> = simulation
            .state()
            .store(ids.flight)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, flight)| {
                        let start = flight.flight.control_points()[0];
                        (entity, [start.x, start.y, start.z])
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (entity, start) in flights {
            if !seen.contains(&entity) {
                seen.push(entity);
                starts.push(start);
            }
        }
        if starts.len() >= 8 {
            break;
        }
    }
    assert!(
        starts.len() >= 8,
        "only {} shot(s) were fired",
        starts.len()
    );
    let feet = RotwkRules::feet(simulation.state(), ids, units[0]);
    let bones = [[200.0, 210.0], [195.0, 210.0]];
    let mut used = [false; 2];
    for start in &starts {
        let at = bones
            .iter()
            .position(|[x, y]| (start[0] - x).abs() < 1e-3 && (start[1] - y).abs() < 1e-3);
        let Some(at) = at else {
            panic!("a shot left from ({}, {}), on no bone", start[0], start[1]);
        };
        used[at] = true;
        assert!((start[2] - (feet + 20.0)).abs() < 1e-3, "z {}", start[2]);
    }
    assert_eq!(used, [true, true], "8 shots all left by one bone");
}

#[test]
fn a_shot_leaves_from_the_bone_its_firing_state_poses() {
    let first_start = |poses: Vec<crate::perches::FiringPose>| {
        let (combat, bow, armour) = combat_data();
        let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
        let shooter = Spawn {
            kind: Some(crate::unit::Kind(0)),
            ..armed_spawn(200.0, 200.0, PLAYER, Some(bow), Some(armour), 100.0)
        };
        let victim = armed_spawn(200.0, 350.0, OTHER, None, Some(armour), 1.0e6);
        let bone = |x: f32, z: f32| crate::perches::Perch {
            position: [x, 0.0, z],
            facing: Facing::EAST,
        };
        let launch = crate::perches::SlotLaunch {
            rest: vec![bone(10.0, 20.0)],
            poses: poses
                .into_iter()
                .map(|mut pose| {
                    pose.bones = vec![bone(4.0, 60.0)];
                    pose
                })
                .collect(),
        };
        let mut simulation = Simulation::new(
            11,
            RotwkRules::new(passability, vec![shooter, victim])
                .with_combat(combat)
                .with_launch_bones(vec![vec![launch]]),
        )
        .unwrap();
        let ids = simulation.rules().ids().unwrap();
        let units: Vec<Entity> = simulation
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![units[0]],
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: units[1] })
            .unwrap();
        for _ in 0..3000 {
            simulation.tick().unwrap();
            let start = simulation.state().store(ids.flight).and_then(|store| {
                store.iter().next().map(|(_, flight)| {
                    let start = flight.flight.control_points()[0];
                    [start.x, start.y, start.z]
                })
            });
            if let Some(start) = start {
                let feet = RotwkRules::feet(simulation.state(), ids, units[0]);
                return [start[0], start[1], start[2] - feet];
            }
        }
        panic!("never fired");
    };
    let pose = |requires: &[&str]| crate::perches::FiringPose {
        requires: requires.iter().map(|token| (*token).to_owned()).collect(),
        bones: Vec::new(),
    };
    let posed = first_start(vec![pose(&["NEVER_HELD"]), pose(&["ATTACKING"])]);
    assert!(
        (posed[1] - 204.0).abs() < 1e-3 && (posed[2] - 60.0).abs() < 1e-3,
        "left from {posed:?}"
    );
    let rest = first_start(vec![pose(&["NEVER_HELD"])]);
    assert!(
        (rest[1] - 210.0).abs() < 1e-3 && (rest[2] - 20.0).abs() < 1e-3,
        "left from {rest:?}"
    );
}
