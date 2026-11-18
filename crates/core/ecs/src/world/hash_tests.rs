#![allow(clippy::arithmetic_side_effects)]

use super::*;
use crate::store::join_mut;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).unwrap()).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Wide {
    byte: u8,
    half: u16,
    word: u32,
    long: u64,
    signed: i32,
    single: f32,
    double: f64,
    flag: bool,
    list: Vec<u16>,
    maybe: Option<u32>,
}

const WIDE_FIELDS: usize = 10;

impl Component for Wide {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.byte);
        hasher.write_u16(self.half);
        hasher.write_u32(self.word);
        hasher.write_u64(self.long);
        hasher.write_i32(self.signed);
        hasher.write_f32(self.single);
        hasher.write_f64(self.double);
        hasher.write_bool(self.flag);
        hasher.write_usize(self.list.len());
        for item in &self.list {
            hasher.write_u16(*item);
        }
        hasher.write_bool(self.maybe.is_some());
        if let Some(value) = self.maybe {
            hasher.write_u32(value);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Tag(u8);

impl Component for Tag {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.0);
    }
}

#[allow(clippy::cast_possible_truncation)]
fn wide(rng: &mut Lcg) -> Wide {
    Wide {
        byte: rng.next() as u8,
        half: rng.next() as u16,
        word: rng.next() as u32,
        long: rng.next(),
        signed: rng.next() as i32,
        single: f32::from_bits((rng.next() as u32) & 0x3fff_ffff),
        double: f64::from_bits(rng.next() & 0x3fff_ffff_ffff_ffff),
        flag: rng.next() % 2 == 0,
        list: (0..rng.below(4)).map(|_| rng.next() as u16).collect(),
        maybe: (rng.next() % 2 == 0).then(|| rng.next() as u32),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn change_one_field(value: &mut Wide, field: usize, rng: &mut Lcg) {
    let bit = rng.next() % 8;
    match field {
        0 => value.byte ^= 1 << bit,
        1 => value.half ^= 1 << (rng.next() % 16),
        2 => value.word ^= 1 << (rng.next() % 32),
        3 => value.long ^= 1 << (rng.next() % 64),
        4 => value.signed ^= 1 << (rng.next() % 32),
        5 => {
            let old = value.single.to_bits();
            value.single = f32::from_bits((old ^ (1 << (rng.next() % 30))) & 0x3fff_ffff);
        }
        6 => {
            let old = value.double.to_bits();
            value.double = f64::from_bits((old ^ (1 << (rng.next() % 62))) & 0x3fff_ffff_ffff_ffff);
        }
        7 => value.flag = !value.flag,
        8 => {
            if value.list.is_empty() || rng.next() % 2 == 0 {
                value.list.push(rng.next() as u16);
            } else {
                let at = rng.below(value.list.len());
                value.list[at] ^= 1 << (rng.next() % 16);
            }
        }
        _ => {
            value.maybe = match value.maybe {
                Some(old) if rng.next() % 2 == 0 => Some(old ^ (1 << (rng.next() % 32))),
                Some(_) => None,
                None => Some(rng.next() as u32),
            };
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum WritePath {
    WorldGetMut,
    StoreGetMut,
    IterMutNth,
    IterMutWhere,
    JoinMut,
    InsertReplacing,
}

const WRITE_PATHS: [WritePath; 6] = [
    WritePath::WorldGetMut,
    WritePath::StoreGetMut,
    WritePath::IterMutNth,
    WritePath::IterMutWhere,
    WritePath::JoinMut,
    WritePath::InsertReplacing,
];

fn write_one_field(
    world: &mut World,
    ids: (ComponentId<Wide>, ComponentId<Tag>),
    target: Entity,
    path: WritePath,
    rng: &mut Lcg,
) {
    let (wide_id, tag_id) = ids;
    let field = rng.below(WIDE_FIELDS);
    match path {
        WritePath::WorldGetMut => {
            change_one_field(world.get_mut(wide_id, target).unwrap(), field, rng)
        }
        WritePath::StoreGetMut => {
            let store = world.store_mut(wide_id).unwrap();
            change_one_field(store.get_mut(target).unwrap(), field, rng);
        }
        WritePath::IterMutNth => {
            let store = world.store_mut(wide_id).unwrap();
            let position = store.entities().iter().position(|e| *e == target).unwrap();
            let (_, value) = store.iter_mut().nth(position).unwrap();
            change_one_field(value, field, rng);
        }
        WritePath::IterMutWhere => {
            let store = world.store_mut(wide_id).unwrap();
            let marker = store.get(target).unwrap().clone();
            let mut hits = store.iter_mut_where(|value| *value == marker);
            let (entity, value) = hits.next().unwrap();
            assert_eq!(entity, target, "the fuzz values are distinct");
            change_one_field(value, field, rng);
        }
        WritePath::JoinMut => {
            let (store, tags) = world.stores_mut(wide_id, tag_id).unwrap();
            let (_, value, _) = join_mut(store, tags)
                .find(|(entity, _, _)| *entity == target)
                .unwrap();
            change_one_field(value, field, rng);
        }
        WritePath::InsertReplacing => {
            let mut value = world.get(wide_id, target).unwrap().clone();
            change_one_field(&mut value, field, rng);
            world.insert(wide_id, target, value).unwrap();
        }
    }
}

#[test]
fn a_one_field_write_through_any_path_changes_the_incremental_hash() {
    let mut rng = Lcg(0x00c0_ffee);
    let mut world = World::new();
    let wide_id = world.register::<Wide>();
    let tag_id = world.register::<Tag>();
    let ids = (wide_id, tag_id);
    let mut live: Vec<Entity> = Vec::new();
    for index in 0..700u32 {
        let entity = world.spawn().unwrap();
        world.insert(wide_id, entity, wide(&mut rng)).unwrap();
        if index % 3 == 0 {
            world.insert(tag_id, entity, Tag(1)).unwrap();
        }
        live.push(entity);
    }
    assert!(live.len() > 64 * 8, "the world must span many hash pages");

    let mut hash = world.state_hash_incremental();
    assert_eq!(hash, world.state_hash());
    for round in 0..3000 {
        let path = WRITE_PATHS[round % WRITE_PATHS.len()];
        let candidates: Vec<Entity> = match path {
            WritePath::JoinMut => world.store(tag_id).unwrap().entities().to_vec(),
            _ => world.store(wide_id).unwrap().entities().to_vec(),
        };
        let target = candidates[rng.below(candidates.len())];
        write_one_field(&mut world, ids, target, path, &mut rng);
        let after = world.state_hash_incremental();
        assert_ne!(
            after, hash,
            "round {round}: {path:?} on {target:?} left the hash unchanged"
        );
        assert_eq!(
            after,
            world.state_hash(),
            "round {round}: {path:?} on {target:?}"
        );

        match rng.below(8) {
            0 => {
                let entity = world.spawn().unwrap();
                world.insert(wide_id, entity, wide(&mut rng)).unwrap();
                live.push(entity);
            }
            1 if live.len() > 600 => {
                let gone = live.swap_remove(rng.below(live.len()));
                world.despawn(gone);
            }
            2 if live.len() > 600 => {
                let gone: Vec<Entity> = (0..3)
                    .map(|_| live.swap_remove(rng.below(live.len())))
                    .collect();
                world.despawn_many(&gone);
            }
            3 => {
                let entity = live[rng.below(live.len())];
                if world.remove(tag_id, entity).is_none() {
                    world.insert(tag_id, entity, Tag(2)).unwrap();
                }
            }
            _ => {}
        }
        if round % 1000 == 500 {
            world.store_mut(tag_id).unwrap().clear();
            for _ in 0..5 {
                let entity = live[rng.below(live.len())];
                world.insert(tag_id, entity, Tag(3)).unwrap();
            }
        }
        hash = world.state_hash_incremental();
        assert_eq!(
            hash,
            world.state_hash(),
            "round {round}: after the structural write"
        );
    }
}

#[test]
fn a_mutable_borrow_that_writes_nothing_leaves_the_hash_alone() {
    let mut rng = Lcg(7);
    let mut world = World::new();
    let wide_id = world.register::<Wide>();
    let entities: Vec<Entity> = (0..200).map(|_| world.spawn().unwrap()).collect();
    for entity in &entities {
        world.insert(wide_id, *entity, wide(&mut rng)).unwrap();
    }
    let before = world.state_hash_incremental();
    let _ = world.get_mut(wide_id, entities[150]);
    for _ in world.store_mut(wide_id).unwrap().iter_mut() {}
    assert_eq!(world.state_hash_incremental(), before);
}

#[test]
fn a_restored_snapshot_brings_a_cache_that_matches_its_state() {
    let mut rng = Lcg(42);
    let mut world = World::new();
    let wide_id = world.register::<Wide>();
    let tag_id = world.register::<Tag>();
    let entities: Vec<Entity> = (0..300).map(|_| world.spawn().unwrap()).collect();
    for entity in &entities {
        world.insert(wide_id, *entity, wide(&mut rng)).unwrap();
        world.insert(tag_id, *entity, Tag(0)).unwrap();
    }

    let original = world.state_hash_incremental();
    let clean = world.snapshot();
    for (round, entity) in entities.iter().enumerate().step_by(37) {
        change_one_field(
            world.get_mut(wide_id, *entity).unwrap(),
            round % WIDE_FIELDS,
            &mut rng,
        );
    }
    world.despawn(entities[5]);
    let extra = world.spawn().unwrap();
    world.insert(tag_id, extra, Tag(9)).unwrap();
    let moved = world.state_hash_incremental();
    assert_ne!(moved, original);

    world.restore(&clean);
    assert_eq!(world.state_hash_incremental(), original);
    assert_eq!(world.state_hash(), original);

    change_one_field(world.get_mut(wide_id, entities[200]).unwrap(), 3, &mut rng);
    let dirty = world.snapshot();
    let dirty_hash = world.state_hash();
    change_one_field(world.get_mut(wide_id, entities[201]).unwrap(), 3, &mut rng);
    assert_ne!(world.state_hash_incremental(), dirty_hash);
    world.restore(&dirty);
    assert_eq!(world.state_hash_incremental(), dirty_hash);
}

#[test]
fn the_incremental_hash_ignores_history_that_left_no_state() {
    let build = |churn: bool| {
        let mut world = World::new();
        let tag_id = world.register::<Tag>();
        let mut kept = Vec::new();
        let mut doomed = Vec::new();
        for index in 0..400u32 {
            let entity = world.spawn().unwrap();
            if index < 100 {
                world.insert(tag_id, entity, Tag(1)).unwrap();
                kept.push(entity);
            } else {
                if churn {
                    world.insert(tag_id, entity, Tag(2)).unwrap();
                }
                doomed.push(entity);
            }
        }
        world.state_hash_incremental();
        world.despawn_many(&doomed);
        world.state_hash_incremental()
    };
    assert_eq!(build(true), build(false));
}
