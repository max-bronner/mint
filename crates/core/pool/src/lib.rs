#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

use std::sync::OnceLock;

use rayon::prelude::*;

static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();

fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |threads| threads.get().saturating_sub(1))
        .max(1)
}

fn pool() -> Option<&'static rayon::ThreadPool> {
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count())
            .thread_name(|index| format!("mint-pool-{index}"))
            .build()
            .ok()
    })
    .as_ref()
}

#[must_use]
pub fn threads() -> usize {
    pool().map_or(0, rayon::ThreadPool::current_num_threads)
}

pub fn map_ordered<T, R, F>(items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync + Send,
{
    match pool() {
        Some(pool) if items.len() > 1 => pool.install(|| items.par_iter().map(&f).collect()),
        _ => items.iter().map(f).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_come_back_in_input_order_however_long_each_item_takes() {
        let items: Vec<u64> = (0..2_000).collect();
        let work = |n: &u64| {
            let mut x = *n;
            for _ in 0..(2_000u64.saturating_sub(*n)) {
                x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            }
            x
        };
        let serial: Vec<u64> = items.iter().map(work).collect();
        assert_eq!(map_ordered(&items, work), serial);
        assert!(threads() >= 1);
    }
}
