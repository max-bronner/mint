use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::water_tuning::ParsedTuning;
use crate::WaterTuning;

const POLL_INTERVAL: Duration = Duration::from_millis(250);

type Reread = Result<ParsedTuning, String>;

pub struct TuningFile {
    path: PathBuf,
    tuning: WaterTuning,
    rereads: Option<mpsc::Receiver<Reread>>,
    stop: Arc<AtomicBool>,
}

impl TuningFile {
    #[must_use]
    pub fn open(path: PathBuf) -> Self {
        let defaults = WaterTuning::default();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    println!("  tuning: could not create {}: {error}", parent.display());
                }
            }
            match std::fs::write(&path, defaults.default_text()) {
                Ok(()) => println!("  tuning: wrote the default file"),
                Err(error) => println!("  tuning: could not write {}: {error}", path.display()),
            }
        }
        println!("  tuning: {}", path.display());
        println!("    edit it while this runs - it is re-read whenever it changes");
        let stamp = stamp_of(&path);
        let mut tuning = defaults;
        match read(&path) {
            Ok(parsed) => {
                report_warnings(&parsed.warnings);
                let changes = defaults.differences(&parsed.tuning);
                tuning = parsed.tuning;
                if changes.is_empty() {
                    println!("    every value is the default, so nothing is changed");
                } else {
                    println!("    {} value(s) differ from the default:", changes.len());
                    for line in &changes {
                        println!("      {line}");
                    }
                }
            }
            Err(error) => {
                println!("  tuning: {}: {error}", path.display());
                println!("    could not be read; the built-in defaults are in force");
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        let rereads = watch(path.clone(), stamp, Arc::clone(&stop));
        Self {
            path,
            tuning,
            rereads,
            stop,
        }
    }

    #[must_use]
    pub fn tuning(&self) -> &WaterTuning {
        &self.tuning
    }

    pub fn poll(&mut self) -> Option<Vec<String>> {
        let newest = self.rereads.as_ref()?.try_iter().last()?;
        match newest {
            Ok(parsed) => {
                report_warnings(&parsed.warnings);
                let changes = self.tuning.differences(&parsed.tuning);
                self.tuning = parsed.tuning;
                Some(changes)
            }
            Err(error) => {
                println!("  tuning: {}: {error}", self.path.display());
                None
            }
        }
    }
}

impl Drop for TuningFile {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn watch(
    path: PathBuf,
    mut stamp: Option<SystemTime>,
    stop: Arc<AtomicBool>,
) -> Option<mpsc::Receiver<Reread>> {
    let (tx, rx) = mpsc::channel();
    let started = std::thread::Builder::new()
        .name("mint-tuning-watch".to_string())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(POLL_INTERVAL);
                let now = stamp_of(&path);
                if now == stamp {
                    continue;
                }
                stamp = now;
                if tx.send(read(&path)).is_err() {
                    break;
                }
            }
        });
    match started {
        Ok(_) => Some(rx),
        Err(error) => {
            println!("  tuning: could not start the watcher ({error}); edits are not picked up");
            None
        }
    }
}

fn stamp_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

fn read(path: &Path) -> Reread {
    std::fs::read_to_string(path)
        .map(|text| WaterTuning::parse(&text))
        .map_err(|error| error.to_string())
}

fn report_warnings(warnings: &[String]) {
    for warning in warnings {
        println!("  tuning: {warning}");
    }
}

#[must_use]
pub fn default_tuning_path(file_name: &str) -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("mint").join(file_name)
}
