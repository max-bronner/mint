#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use rayon::prelude::*;

type Workers = Option<Arc<rayon::ThreadPool>>;

static POOL: OnceLock<Workers> = OnceLock::new();

thread_local! {
    static SCOPED: RefCell<Option<Workers>> = const { RefCell::new(None) };
}

fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |threads| threads.get().saturating_sub(1))
        .max(1)
}

fn build(threads: usize) -> Workers {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|index| format!("mint-pool-{index}"))
        .build()
        .ok()
        .map(Arc::new)
}

fn pool() -> Workers {
    SCOPED
        .with(|scoped| scoped.borrow().clone())
        .unwrap_or_else(|| POOL.get_or_init(|| build(worker_count())).clone())
}

#[must_use]
pub fn threads() -> usize {
    pool().map_or(0, |pool| pool.current_num_threads())
}

pub fn with_threads<R>(threads: usize, f: impl FnOnce() -> R) -> Option<R> {
    let workers = if threads > 1 {
        Some(build(threads)?)
    } else {
        None
    };
    struct Restore(Option<Workers>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            SCOPED.with(|scoped| *scoped.borrow_mut() = previous);
        }
    }
    let previous = SCOPED.with(|scoped| scoped.borrow_mut().replace(workers));
    let _restore = Restore(previous);
    Some(f())
}

pub fn set_threads(threads: usize) -> bool {
    let mut set = false;
    POOL.get_or_init(|| {
        set = true;
        if threads > 1 {
            build(threads)
        } else {
            None
        }
    });
    set
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

#[must_use = "a background job's result is only taken by `wait`"]
pub struct Background<T> {
    result: std::sync::mpsc::Receiver<std::thread::Result<T>>,
}

impl<T> Background<T> {
    pub fn wait(self) -> T {
        match self.result.recv() {
            Ok(Ok(value)) => value,
            Ok(Err(payload)) => std::panic::resume_unwind(payload),
            Err(_) => std::panic::resume_unwind(Box::new("a background job was dropped unrun")),
        }
    }
}

pub fn background<T, F>(f: F) -> Background<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (send, result) = std::sync::mpsc::channel();
    let job = move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let _ = send.send(outcome);
    };
    match pool() {
        Some(pool) => pool.spawn(job),
        None => job(),
    }
    Background { result }
}

pub fn map_chunks_mut<T, R, F>(items: &mut [T], chunk_len: usize, f: F) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(usize, &mut [T]) -> R + Sync + Send,
{
    let chunk_len = chunk_len.max(1);
    let pool = match pool() {
        Some(pool) if items.len() > chunk_len => pool,
        _ => {
            return items
                .chunks_mut(chunk_len)
                .enumerate()
                .map(|(index, chunk)| f(index.saturating_mul(chunk_len), chunk))
                .collect();
        }
    };
    let chunks: Vec<Mutex<Option<&mut [T]>>> = items
        .chunks_mut(chunk_len)
        .map(|chunk| Mutex::new(Some(chunk)))
        .collect();
    let results: Vec<Mutex<Option<R>>> = chunks.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let drain = || loop {
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some(slot) = chunks.get(index) else {
            break;
        };
        let Some(chunk) = slot.lock().ok().and_then(|mut held| held.take()) else {
            continue;
        };
        let answer = f(index.saturating_mul(chunk_len), chunk);
        if let Some(mut out) = results.get(index).and_then(|slot| slot.lock().ok()) {
            *out = Some(answer);
        }
    };
    pool.in_place_scope(|scope| {
        scope.spawn_broadcast(|_, _| drain());
        drain();
    });
    results
        .into_iter()
        .filter_map(|slot| slot.into_inner().ok().flatten())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chunk_is_visited_once_at_its_own_offset_and_answers_in_order() {
        for chunk_len in [0usize, 1, 7, 64, 999, 1000, 5000] {
            let mut items: Vec<usize> = vec![0; 1000];
            let starts = map_chunks_mut(&mut items, chunk_len, |offset, chunk| {
                for (at, item) in chunk.iter_mut().enumerate() {
                    *item = item.wrapping_add(offset.wrapping_add(at).wrapping_add(1));
                }
                offset
            });
            assert!(
                items.iter().zip(1usize..).all(|(item, want)| *item == want),
                "{chunk_len}"
            );
            let step = chunk_len.max(1);
            let want: Vec<usize> = (0..1000).step_by(step).collect();
            assert_eq!(starts, want, "{chunk_len}");
        }
    }

    #[test]
    fn a_background_job_overlaps_the_caller_and_may_use_the_pool_itself() {
        let job = background(|| {
            let items: Vec<u64> = (0..500).collect();
            map_ordered(&items, |n| n.wrapping_mul(3))
                .iter()
                .sum::<u64>()
        });
        let own = map_ordered(&[1u64, 2, 3], |n| n.wrapping_add(1));
        assert_eq!(own, [2, 3, 4]);
        assert_eq!(job.wait(), 374_250);
    }

    #[test]
    fn a_panic_in_a_background_job_resumes_in_wait_not_in_the_pool() {
        let job = background(|| -> u32 { std::panic::resume_unwind(Box::new("boom")) });
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.wait()));
        assert!(caught.is_err());
        assert_eq!(background(|| 7).wait(), 7);
    }

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

    #[test]
    fn with_threads_sets_this_threads_pool_for_its_call_only() {
        let engine = threads();
        assert_eq!(with_threads(1, threads), Some(0));
        assert_eq!(with_threads(3, threads), Some(3));
        assert_eq!(with_threads(3, || with_threads(0, threads)), Some(Some(0)));
        assert_eq!(threads(), engine);
        let items: Vec<u32> = (0..100).collect();
        let doubled = |n: &u32| n.wrapping_mul(2);
        assert_eq!(
            with_threads(4, || map_ordered(&items, doubled)),
            Some(items.iter().map(doubled).collect())
        );
    }
}
