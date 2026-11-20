use std::sync::OnceLock;
use std::time::Instant;

static EPOCH: OnceLock<Instant> = OnceLock::new();

pub(crate) fn begin() {
    EPOCH.get_or_init(Instant::now);
}

pub(crate) fn mark(what: &str) {
    if let Some(epoch) = EPOCH.get() {
        println!(
            "startup: {what} at {:.0} ms",
            epoch.elapsed().as_secs_f64() * 1000.0
        );
    }
}
