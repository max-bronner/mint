use std::path::PathBuf;
use std::time::SystemTime;

use crate::WaterTuning;

pub struct TuningFile {
    path: PathBuf,
    stamp: Option<SystemTime>,
    tuning: WaterTuning,
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
        let mut file = Self {
            path,
            stamp: None,
            tuning: defaults,
        };
        println!("  tuning: {}", file.path.display());
        println!("    edit it while this runs - it is re-read whenever it changes");
        match file.read() {
            Some(parsed) => {
                let changes = defaults.differences(&parsed);
                file.tuning = parsed;
                if changes.is_empty() {
                    println!("    every value is the default, so nothing is changed");
                } else {
                    println!("    {} value(s) differ from the default:", changes.len());
                    for line in &changes {
                        println!("      {line}");
                    }
                }
            }
            None => println!("    could not be read; the built-in defaults are in force"),
        }
        file
    }

    #[must_use]
    pub fn tuning(&self) -> &WaterTuning {
        &self.tuning
    }

    pub fn poll(&mut self) -> Option<Vec<String>> {
        let stamp = std::fs::metadata(&self.path)
            .and_then(|meta| meta.modified())
            .ok();
        if stamp == self.stamp {
            return None;
        }
        self.stamp = stamp;
        let parsed = self.read()?;
        let changes = self.tuning.differences(&parsed);
        self.tuning = parsed;
        Some(changes)
    }

    fn read(&mut self) -> Option<WaterTuning> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) => {
                println!("  tuning: {}: {error}", self.path.display());
                return None;
            }
        };
        self.stamp = std::fs::metadata(&self.path)
            .and_then(|meta| meta.modified())
            .ok();
        let parsed = WaterTuning::parse(&text);
        for warning in &parsed.warnings {
            println!("  tuning: {warning}");
        }
        Some(parsed.tuning)
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
