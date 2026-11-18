use super::*;
use crate::rules::unit_index::UnitIndex;

fn border_layout() -> (Simulation<RotwkRules>, Ids) {
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let mut spawns = Vec::new();
    for step in 0..16u8 {
        for offset in [-0.01f32, 0.0, 0.01] {
            let at = f32::from(step) * 40.0 + offset;
            spawns.push(spawn_at(at, 200.0, KindOf::SELECTABLE, PLAYER));
            spawns.push(spawn_at(200.0, at, KindOf::SELECTABLE, OTHER));
            spawns.push(spawn_at(at, at, KindOf::SELECTABLE, OTHER));
        }
    }
    spawns.push(spawn_at(-300.0, 900.0, KindOf::SELECTABLE, OTHER));
    for i in 0..400u16 {
        let at = Vec2::new(
            5.0 + f32::from(i % 100) * 6.0,
            500.0 + f32::from(i / 100) * 20.0,
        );
        spawns.push(spawn_at(at.x, at.y, KindOf::SELECTABLE, PLAYER));
    }
    let simulation = Simulation::new(3, RotwkRules::new(passability, spawns)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn printed<T: core::fmt::Debug>(value: &T) -> String {
    format!("{value:?}")
}

#[test]
fn the_bucket_walk_keeps_what_the_full_walk_keeps_on_bucket_borders() {
    let (simulation, ids) = border_layout();
    let rules = simulation.rules();
    let world = simulation.state();
    let mut index = UnitIndex::default();
    index.begin();
    assert!(index.sync(rules, world, ids));
    let mut boundary_hits = 0;
    for centre in [
        Vec2::new(200.0, 200.0),
        Vec2::new(160.0, 240.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(-50.0, 700.0),
        Vec2::new(333.3, 81.7),
    ] {
        for tenths in 0..=40u16 {
            let r = f32::from(tenths) * 10.0;
            let reach_sq = r * r;
            let indexed: Vec<_> = index
                .query(centre, reach_sq)
                .unwrap()
                .into_iter()
                .copied()
                .collect();
            let full = rules.bodies_near(world, ids, centre, reach_sq);
            assert_eq!(
                printed(&indexed),
                printed(&full),
                "round {centre:?} within {r}"
            );
            boundary_hits += full
                .iter()
                .filter(|record| record.position.minus(centre).length_squared() == reach_sq)
                .count();
        }
    }
    assert!(
        boundary_hits > 20,
        "only {boundary_hits} bodies sat on a query's edge"
    );
}

#[test]
fn the_refreshed_index_equals_a_rebuild_after_writes_to_its_inputs() {
    let (simulation, ids) = border_layout();
    let rules = simulation.rules();
    let mut world = World::new();
    world.restore(&simulation.state().snapshot());
    let mut index = UnitIndex::default();
    index.begin();
    assert!(index.sync(rules, &world, ids));
    let bodies: Vec<Entity> = world.store(ids.body).unwrap().entities().to_vec();
    let shift = |world: &mut World, entity: Entity, by: Vec2| {
        let body = world.get_mut(ids.body, entity).unwrap();
        body.0.position = body.0.position.plus(by);
    };
    shift(&mut world, bodies[3], Vec2::new(130.0, -75.0));
    shift(&mut world, bodies[4], Vec2::new(1.0, 1.0));
    world
        .insert(
            ids.doomed,
            bodies[100],
            crate::combat::Doomed {
                destroy_at: Frame::new(9),
                death_index: 0,
                death_conditions: mint_sage_scene::Conditions::default(),
            },
        )
        .unwrap();
    world
        .insert(ids.concealed, bodies[200], crate::construction::Concealed)
        .unwrap();
    world
        .insert(ids.owner, bodies[300], crate::unit::Owner(PLAYER))
        .unwrap();
    world.remove(ids.shape, bodies[400]);
    let born = world.spawn().unwrap();
    let shape = *world.get(ids.shape, bodies[0]).unwrap();
    let mut body = *world.get(ids.body, bodies[0]).unwrap();
    body.0.position = Vec2::new(205.0, 195.0);
    world.insert(ids.shape, born, shape).unwrap();
    world.insert(ids.body, born, body).unwrap();
    assert!(index.sync(rules, &world, ids));

    let mut rebuilt = UnitIndex::default();
    rebuilt.begin();
    assert!(rebuilt.sync(rules, &world, ids));
    assert_eq!(printed(&index.records()), printed(&rebuilt.records()));
    for centre in [Vec2::new(200.0, 200.0), Vec2::new(250.0, 120.0)] {
        for r in [50.0f32, 150.0, 400.0] {
            let full = rules.bodies_near(&world, ids, centre, r * r);
            assert_eq!(
                printed(
                    &index
                        .query(centre, r * r)
                        .unwrap()
                        .into_iter()
                        .copied()
                        .collect::<Vec<_>>()
                ),
                printed(&full)
            );
        }
    }
}
