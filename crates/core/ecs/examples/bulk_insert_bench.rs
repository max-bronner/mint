use std::time::Instant;

use mint_ecs::{Component, ComponentStore, Entity, StateHasher, World};

#[derive(Debug, Clone, Copy)]
struct Marker(u32);

impl Component for Marker {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0);
    }
}

const SIZES: &[usize] = &[500, 2_000, 8_000, 32_000];

fn main() {
    println!("n         insert (descending arrival)   from_sorted        ratio");
    let mut previous: Option<(f64, f64)> = None;
    for &n in SIZES {
        let entities = spawn_n(n);

        let mut descending = entities.clone();
        descending.reverse();
        let insert_seconds = time(|| {
            let mut store: ComponentStore<Marker> = ComponentStore::new();
            for &entity in &descending {
                store.insert(entity, Marker(entity.index()));
            }
            store
        });

        let pairs: Vec<(Entity, Marker)> = entities
            .iter()
            .map(|&entity| (entity, Marker(entity.index())))
            .collect();
        let sorted_seconds = time(|| ComponentStore::from_sorted(pairs));

        let ratio = if sorted_seconds > 0.0 {
            insert_seconds / sorted_seconds
        } else {
            f64::INFINITY
        };
        println!(
            "{n:<9} {insert_seconds:>10.6}s                {sorted_seconds:>10.6}s   {ratio:>6.1}x"
        );

        if let Some((prev_insert, prev_sorted)) = previous {
            let insert_growth = insert_seconds / prev_insert.max(f64::EPSILON);
            let sorted_growth = sorted_seconds / prev_sorted.max(f64::EPSILON);
            println!(
                "  growth since previous size (4x n): insert {insert_growth:.1}x, from_sorted {sorted_growth:.1}x"
            );
        }
        previous = Some((insert_seconds, sorted_seconds));
    }
}

fn spawn_n(n: usize) -> Vec<Entity> {
    let mut world = World::new();
    let mut entities = Vec::with_capacity(n);
    for _ in 0..n {
        match world.spawn() {
            Ok(entity) => entities.push(entity),
            Err(error) => {
                eprintln!("spawn failed at {} entities: {error}", entities.len());
                break;
            }
        }
    }
    entities
}

fn time<T>(f: impl FnOnce() -> T) -> f64 {
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed().as_secs_f64();
    drop(result);
    elapsed
}
