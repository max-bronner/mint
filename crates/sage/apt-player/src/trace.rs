use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

pub type Sink = Box<dyn Fn(&str, &[String], &str) + Send + Sync>;

static ENABLED: AtomicBool = AtomicBool::new(false);
static SINK: OnceLock<Mutex<Option<Sink>>> = OnceLock::new();

pub fn set_sink(sink: Option<Sink>) {
    ENABLED.store(sink.is_some(), Ordering::Release);
    *SINK
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = sink;
}

#[must_use]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub(crate) fn fire(name: &str, args: &[String], result: &str) {
    if !enabled() {
        return;
    }
    let Some(lock) = SINK.get() else {
        return;
    };
    if let Some(sink) = &*lock.lock().unwrap_or_else(PoisonError::into_inner) {
        sink(name, args, result);
    }
}
