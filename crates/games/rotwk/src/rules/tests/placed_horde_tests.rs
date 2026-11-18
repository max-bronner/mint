use super::*;

#[test]
fn a_placed_horde_is_created_with_its_members_and_its_name() {
    let formation = square_formation();
    let slots = u32::try_from(formation.len()).unwrap();
    let placed = crate::rules::PlacedHorde {
        spawn: HordeSpawn {
            formation: 0,
            origin: Vec2::new(200.0, 200.0),
            facing: Facing::EAST,
            member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER),
            count: slots,
            vision: 0.0,
        },
        kind: crate::unit::Kind(0),
        placement: Some(7),
        name: Some(String::from("Camp Guards")),
        team: None,
        indestructible: false,
        keys: Default::default(),
    };
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(vec![formation], Vec::new())
        .with_placed_hordes(vec![placed]);
    let simulation = Simulation::new(9, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let horde = horde_entity(&simulation, ids).expect("no horde was created");
    assert_eq!(members_of(&simulation, ids).len(), 9);
    assert_eq!(
        simulation
            .state()
            .get(ids.script_name, horde)
            .map(|name| name.0.as_str()),
        Some("Camp Guards")
    );
}

#[test]
fn a_placed_indestructible_horde_has_indestructible_members() {
    let formation = square_formation();
    let slots = u32::try_from(formation.len()).unwrap();
    let placed = crate::rules::PlacedHorde {
        spawn: HordeSpawn {
            formation: 0,
            origin: Vec2::new(200.0, 200.0),
            facing: Facing::EAST,
            member: Spawn {
                health: Some(100.0),
                ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
            },
            count: slots,
            vision: 0.0,
        },
        kind: crate::unit::Kind(0),
        placement: Some(7),
        name: None,
        team: None,
        indestructible: true,
        keys: Default::default(),
    };
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(vec![formation], Vec::new())
        .with_placed_hordes(vec![placed]);
    let simulation = Simulation::new(9, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    assert_eq!(members.len(), 9);
    for member in members {
        let health = simulation.state().get(ids.health, member).unwrap();
        assert!(
            health.0.is_indestructible(),
            "member {member:?} can be hurt"
        );
    }
}

#[test]
fn a_placed_indestructible_object_starts_indestructible() {
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let spawn = |x| Spawn {
        health: Some(100.0),
        ..spawn_at(x, 50.0, KindOf::SELECTABLE, PLAYER)
    };
    let rules =
        RotwkRules::new(passability, vec![spawn(50.0), spawn(90.0)]).with_indestructible(vec![1]);
    let simulation = Simulation::new(9, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let flags: Vec<bool> = simulation
        .state()
        .store(ids.health)
        .unwrap()
        .iter()
        .map(|(_, health)| health.0.is_indestructible())
        .collect();
    assert_eq!(flags, vec![false, true]);
}
