use super::*;
use crate::store::{join, join_mut};

#[derive(Debug, PartialEq, Clone, Copy)]
struct Health(i32);

impl Component for Health {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.0);
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
}

impl Component for Position {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.x);
        hasher.write_f32(self.y);
    }
}

#[test]
fn registering_the_same_type_twice_returns_the_same_id() {
    let mut world = World::new();
    let first = world.register::<Health>();
    let other = world.register::<Position>();
    let again = world.register::<Health>();
    assert_eq!(first, again);
    assert_ne!(first.index(), other.index());
    assert_eq!(world.component_count(), 2);
}

#[test]
fn despawn_many_matches_despawning_one_at_a_time() {
    let build = || {
        let mut world = World::new();
        let health = world.register::<Health>();
        let position = world.register::<Position>();
        let entities: Vec<Entity> = (0..6).map(|_| world.spawn().unwrap()).collect();
        for (i, entity) in entities.iter().enumerate() {
            let value = i32::try_from(i).unwrap();
            world.insert(health, *entity, Health(value)).unwrap();
            if i % 2 == 0 {
                world
                    .insert(position, *entity, Position { x: 1.0, y: 2.0 })
                    .unwrap();
            }
        }
        (world, entities)
    };
    let (mut single, entities) = build();
    let (mut batch, _) = build();
    let doomed = [entities[4], entities[1], entities[4], entities[2]];
    for entity in doomed {
        single.despawn(entity);
    }
    assert_eq!(
        batch.despawn_many(&doomed),
        3,
        "the repeat is not live twice"
    );
    assert_eq!(batch.state_hash(), single.state_hash());
    for _ in 0..3 {
        assert_eq!(batch.spawn().unwrap(), single.spawn().unwrap());
    }
    assert_eq!(batch.despawn_many(&[]), 0);
}

#[test]
fn despawn_removes_the_entity_from_every_store() {
    let mut world = World::new();
    let health = world.register::<Health>();
    let position = world.register::<Position>();
    let a = world.spawn().unwrap();
    let b = world.spawn().unwrap();
    world.insert(health, a, Health(10)).unwrap();
    world
        .insert(position, a, Position { x: 1.0, y: 2.0 })
        .unwrap();
    world.insert(health, b, Health(20)).unwrap();

    assert!(world.despawn(a));
    assert!(!world.despawn(a));
    assert_eq!(world.get(health, a), None);
    assert_eq!(world.get(position, a), None);
    assert_eq!(world.store(health).map(ComponentStore::len), Some(1));
    assert_eq!(world.store(position).map(ComponentStore::len), Some(0));
    assert_eq!(world.get(health, b), Some(&Health(20)));

    let c = world.spawn().unwrap();
    assert_eq!(c.index(), a.index());
    assert_eq!(world.get(health, c), None);
}

#[test]
fn insert_refuses_a_dead_entity() {
    let mut world = World::new();
    let health = world.register::<Health>();
    let entity = world.spawn().unwrap();
    assert!(world.despawn(entity));
    assert_eq!(
        world.insert(health, entity, Health(1)),
        Err(Error::StaleEntity {
            index: entity.index(),
            generation: entity.generation(),
        })
    );
    assert_eq!(world.store(health).map(ComponentStore::len), Some(0));
}

#[test]
fn an_id_from_another_world_is_rejected_rather_than_confused() {
    let mut world = World::new();
    let mut other = World::new();
    let health = world.register::<Health>();
    let entity = other.spawn().unwrap();
    assert_eq!(
        other.insert(health, entity, Health(1)),
        Err(Error::UnknownComponent { index: 0 })
    );
    assert_eq!(other.get(health, entity), None);
}

#[test]
fn joining_two_stores_from_the_world_yields_the_intersection() {
    let mut world = World::new();
    let health = world.register::<Health>();
    let position = world.register::<Position>();
    let a = world.spawn().unwrap();
    let b = world.spawn().unwrap();
    let c = world.spawn().unwrap();
    world.insert(health, a, Health(1)).unwrap();
    world.insert(health, c, Health(3)).unwrap();
    world
        .insert(position, b, Position { x: 0.0, y: 0.0 })
        .unwrap();
    world
        .insert(position, c, Position { x: 5.0, y: 6.0 })
        .unwrap();

    let (Some(healths), Some(positions)) = (world.store(health), world.store(position)) else {
        panic!("both stores are registered");
    };
    let found: Vec<Entity> = join(healths, positions)
        .map(|(entity, _, _)| entity)
        .collect();
    assert_eq!(found, vec![c]);
}

#[test]
fn the_hash_ignores_the_order_components_were_inserted() {
    fn build(reversed: bool) -> u64 {
        let mut world = World::new();
        let health = world.register::<Health>();
        let entities: Vec<Entity> = (0..4).map(|_| world.spawn().unwrap()).collect();
        let mut order: Vec<usize> = (0..entities.len()).collect();
        if reversed {
            order.reverse();
        }
        for i in order {
            let entity = entities[i];
            let value = i32::try_from(i).unwrap_or(-1);
            world.insert(health, entity, Health(value)).unwrap();
        }
        world.state_hash()
    }
    assert_eq!(build(false), build(true));
}

#[test]
fn the_hash_ignores_allocation_history_that_left_no_live_state() {
    let mut plain = World::new();
    let plain_health = plain.register::<Health>();
    let a = plain.spawn().unwrap();
    let b = plain.spawn().unwrap();
    let c = plain.spawn().unwrap();
    assert!(plain.despawn(b));
    plain.insert(plain_health, a, Health(1)).unwrap();
    plain.insert(plain_health, c, Health(3)).unwrap();

    let mut churned = World::new();
    let churned_health = churned.register::<Health>();
    let a2 = churned.spawn().unwrap();
    let b2 = churned.spawn().unwrap();
    let c2 = churned.spawn().unwrap();
    assert!(churned.despawn(b2));
    for _ in 0..2 {
        let temp = churned.spawn().unwrap();
        assert_eq!(temp.index(), b2.index());
        churned.insert(churned_health, temp, Health(99)).unwrap();
        assert!(churned.despawn(temp));
    }
    churned.insert(churned_health, a2, Health(1)).unwrap();
    churned.insert(churned_health, c2, Health(3)).unwrap();

    assert_eq!(plain.state_hash(), churned.state_hash());
}

#[test]
fn the_hash_changes_when_one_field_changes() {
    let mut world = World::new();
    let position = world.register::<Position>();
    let entity = world.spawn().unwrap();
    world
        .insert(position, entity, Position { x: 1.0, y: 2.0 })
        .unwrap();
    let before = world.state_hash();
    if let Some(value) = world.get_mut(position, entity) {
        value.y = 2.000_001;
    }
    assert_ne!(before, world.state_hash());
}

#[test]
fn the_hash_covers_entities_that_own_no_component() {
    let mut world = World::new();
    world.register::<Health>();
    let before = world.state_hash();
    let _entity = world.spawn().unwrap();
    assert_ne!(before, world.state_hash());
}

#[test]
fn the_hash_distinguishes_a_recycled_slot_from_the_original() {
    let mut first = World::new();
    let id = first.register::<Health>();
    let a = first.spawn().unwrap();
    first.insert(id, a, Health(7)).unwrap();

    let mut second = World::new();
    let id2 = second.register::<Health>();
    let b = second.spawn().unwrap();
    assert!(second.despawn(b));
    let b2 = second.spawn().unwrap();
    assert_eq!(b2.index(), b.index());
    second.insert(id2, b2, Health(7)).unwrap();

    assert_ne!(first.state_hash(), second.state_hash());
}

#[test]
fn the_hash_depends_on_registration_order() {
    fn build(health_first: bool) -> u64 {
        let mut world = World::new();
        let (health, position) = if health_first {
            let h = world.register::<Health>();
            (h, world.register::<Position>())
        } else {
            let p = world.register::<Position>();
            (world.register::<Health>(), p)
        };
        let entity = world.spawn().unwrap();
        world.insert(health, entity, Health(3)).unwrap();
        world
            .insert(position, entity, Position { x: 1.0, y: 2.0 })
            .unwrap();
        world.state_hash()
    }
    assert_ne!(build(true), build(false));
}

#[test]
fn the_hash_is_reproduced_by_an_identical_command_sequence() {
    fn run() -> Vec<u64> {
        let mut world = World::new();
        let health = world.register::<Health>();
        let position = world.register::<Position>();
        let mut hashes = Vec::new();
        let mut live: Vec<Entity> = Vec::new();
        for step in 0..12_i32 {
            let entity = world.spawn().unwrap();
            world.insert(health, entity, Health(step)).unwrap();
            if step % 2 == 0 {
                let x = f32::from(i16::try_from(step).unwrap_or(0));
                world
                    .insert(position, entity, Position { x, y: -x })
                    .unwrap();
            }
            live.push(entity);
            if step % 3 == 0 {
                let victim = live.remove(0);
                assert!(world.despawn(victim));
            }
            hashes.push(world.state_hash());
        }
        hashes
    }
    let first = run();
    assert_eq!(first, run());
    assert!(first.windows(2).any(|pair| pair[0] != pair[1]));
}

fn apply_script_step(
    world: &mut World,
    health: ComponentId<Health>,
    position: ComponentId<Position>,
    step: u32,
) {
    if step % 5 == 0 {
        let victim = world.iter_entities().next();
        if let Some(victim) = victim {
            world.despawn(victim);
            return;
        }
    }
    let entity = world.spawn().unwrap();
    let value = i32::try_from(step).unwrap_or(-1);
    world.insert(health, entity, Health(value)).unwrap();
    if step % 3 == 0 {
        let x = f32::from(i16::try_from(value).unwrap_or(0));
        world
            .insert(position, entity, Position { x, y: -x })
            .unwrap();
    }
}

#[test]
fn a_world_restored_from_a_snapshot_continues_identically() {
    const TOTAL_STEPS: u32 = 400;
    const SNAPSHOT_AT: u32 = 200;

    let mut original = World::new();
    let health = original.register::<Health>();
    let position = original.register::<Position>();

    let mut hashes_before: Vec<u64> = Vec::new();
    for step in 0..SNAPSHOT_AT {
        apply_script_step(&mut original, health, position, step);
        hashes_before.push(original.state_hash());
    }
    let snapshot = original.snapshot();
    let hash_at_snapshot = original.state_hash();

    assert!(
        original
            .iter_entities()
            .any(|entity| entity.generation() > 1),
        "the script must reuse at least one slot before the snapshot point"
    );
    let distinct_before: std::collections::BTreeSet<u64> = hashes_before.iter().copied().collect();
    assert!(
        distinct_before.len() > 50,
        "only {} distinct hashes before the snapshot; the script is too repetitive \
             to prove anything",
        distinct_before.len()
    );

    let mut tail_original: Vec<u64> = Vec::new();
    for step in SNAPSHOT_AT..TOTAL_STEPS {
        apply_script_step(&mut original, health, position, step);
        tail_original.push(original.state_hash());
    }

    let mut restored = World::new();
    let restored_health = restored.register::<Health>();
    let restored_position = restored.register::<Position>();
    assert_eq!(restored_health, health, "registration order must match");
    assert_eq!(restored_position, position, "registration order must match");
    restored.restore(&snapshot);

    assert_eq!(
        restored.state_hash(),
        hash_at_snapshot,
        "the restored world must hash identically to the moment it was snapshotted"
    );

    let mut tail_restored: Vec<u64> = Vec::new();
    for step in SNAPSHOT_AT..TOTAL_STEPS {
        apply_script_step(&mut restored, health, position, step);
        tail_restored.push(restored.state_hash());
    }

    assert_eq!(
        tail_original, tail_restored,
        "a restored world must produce the exact same hash sequence as the original \
             continuing from the same point - this is what makes restore usable for \
             render interpolation, save/load, rejoin and desync dumps"
    );
    let tail_distinct: std::collections::BTreeSet<u64> = tail_original.iter().copied().collect();
    assert!(
        tail_distinct.len() > 50,
        "the tail barely changed: {} distinct hashes",
        tail_distinct.len()
    );
}

#[test]
fn restore_carries_the_free_list_so_the_next_spawn_reuses_the_same_slot() {
    let mut world = World::new();
    let health = world.register::<Health>();
    let a = world.spawn().unwrap();
    let b = world.spawn().unwrap();
    let _c = world.spawn().unwrap();
    world.insert(health, a, Health(1)).unwrap();
    assert!(world.despawn(b));
    let snapshot = world.snapshot();
    let next_here = world.spawn().unwrap();

    let mut restored = World::new();
    let restored_health = restored.register::<Health>();
    assert_eq!(restored_health, health);
    restored.restore(&snapshot);
    let next_restored = restored.spawn().unwrap();

    assert_eq!(
        next_here, next_restored,
        "the restored allocator must hand out the same handle the original did"
    );
}

#[test]
fn stores_mut_refuses_to_alias_the_same_store() {
    let mut world = World::new();
    let health = world.register::<Health>();
    assert!(world.stores_mut(health, health).is_none());
}

#[test]
fn stores_mut_returns_none_for_an_id_past_the_end() {
    let mut world = World::new();
    let health = world.register::<Health>();
    let bogus = ComponentId::<Position>::new(9_999);
    assert!(world.stores_mut(health, bogus).is_none());
    assert!(world.stores_mut(bogus, health).is_none());
}

#[test]
fn join_mut_writes_position_while_reading_health_with_no_intermediate_copy() {
    let mut world = World::new();
    let health = world.register::<Health>();
    let position = world.register::<Position>();
    let a = world.spawn().unwrap();
    let b = world.spawn().unwrap();
    let c = world.spawn().unwrap();
    world.insert(health, a, Health(10)).unwrap();
    world
        .insert(position, a, Position { x: 0.0, y: 0.0 })
        .unwrap();
    world.insert(health, b, Health(20)).unwrap();
    world
        .insert(position, c, Position { x: 1.0, y: 1.0 })
        .unwrap();

    let Some((positions, healths)) = world.stores_mut(position, health) else {
        panic!("both stores are registered and distinct");
    };
    for (_entity, pos, hp) in join_mut(positions, healths) {
        pos.x = f32::from(i16::try_from(hp.0).unwrap_or(0));
    }

    assert_eq!(world.get(position, a), Some(&Position { x: 10.0, y: 0.0 }));
    assert_eq!(world.get(position, b), None);
    assert_eq!(
        world.get(position, c),
        Some(&Position { x: 1.0, y: 1.0 }),
        "c has no health, so the join must have left its position untouched"
    );
}
