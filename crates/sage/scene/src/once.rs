use std::collections::BTreeSet;
use std::sync::Mutex;

const MAX_LATCHED: usize = 64;

static LATCHED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

pub(crate) fn report_once(message: &str) {
    let Ok(mut latched) = LATCHED.lock() else {
        return;
    };
    if latched.len() >= MAX_LATCHED || latched.contains(message) {
        return;
    }
    latched.insert(message.to_owned());
    eprintln!("{message}");
}
