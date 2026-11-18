use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct Access {
    pub block_keyword: String,
    pub selector: Option<String>,
    pub field: String,
}

static LOG: Mutex<Vec<Access>> = Mutex::new(Vec::new());

pub fn record(block_keyword: &str, selector: Option<&str>, field: &str) {
    let Ok(mut log) = LOG.lock() else {
        return;
    };
    log.push(Access {
        block_keyword: block_keyword.to_owned(),
        selector: selector.map(str::to_owned),
        field: field.to_owned(),
    });
}

#[must_use]
pub fn take() -> Vec<Access> {
    LOG.lock()
        .map(|mut log| std::mem::take(&mut *log))
        .unwrap_or_default()
}
