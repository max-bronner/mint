use super::*;

fn enemy_horde(
    formation: u16,
    origin: Vec2,
    count: u32,
    armour: mint_sage_combat::ArmorId,
) -> HordeSpawn {
    HordeSpawn {
        formation,
        origin,
        facing: Facing::EAST,
        member: Spawn {
            health: Some(5.0),
            weapon: None,
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, OTHER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    }
}

#[test]
fn a_ranged_unit_that_kills_a_member_scans_back_into_that_horde() {
    let (combat, bow, armour) = combat_data();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let line = table_of(&[
        Vec2::new(-3.333, -70.0),
        Vec2::new(-3.333, 80.0),
        Vec2::new(6.667, -10.0),
    ]);
    let lone = table_of(&[Vec2::ZERO]);
    let rules = RotwkRules::new(
        passability,
        vec![armed_spawn(
            200.0,
            200.0,
            PLAYER,
            Some(bow),
            Some(armour),
            10_000.0,
        )],
    )
    .with_combat(combat)
    .with_hordes(
        vec![line, lone],
        vec![
            enemy_horde(0, Vec2::new(343.333, 270.0), 3, armour),
            enemy_horde(1, Vec2::new(200.0, 350.0), 1, armour),
        ],
    );
    let mut simulation = Simulation::new(31, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let shooter = simulation
        .state()
        .store(ids.armed)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    let mut taken: Vec<Entity> = Vec::new();
    for _ in 0..900 {
        simulation.tick().unwrap();
        let target = simulation
            .state()
            .get(ids.armed, shooter)
            .and_then(|armed| armed.target);
        if let Some(target) = target.filter(|target| taken.last() != Some(target)) {
            taken.push(target);
        }
        if taken.len() == 2 {
            break;
        }
    }
    let [_, second] = taken[..] else {
        panic!("two targets expected, got {}", taken.len());
    };
    let at = simulation.state().get(ids.body, second).unwrap().0.position;
    assert!(
        (at.x - 350.0).abs() < 1.0 && (at.y - 260.0).abs() < 1.0,
        "the second target stands at {at:?}"
    );
}
