use std::any::Any;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const MAIN_FILE: &str = "crash.txt";
pub(crate) const REPLAY_FILE: &str = "replay.txt";

const MAX_SECTION_BYTES: usize = 1 << 20;
const MAX_NOTES: usize = 64;

pub(crate) struct Report {
    dir: PathBuf,
    out: Option<BufWriter<File>>,
    notes: Vec<String>,
}

impl Report {
    pub(crate) fn create(root: &Path, name: &str) -> Option<Self> {
        let fallback = std::env::temp_dir().join("mint-crash-dumps");
        let mut notes = Vec::new();
        for base in [root, fallback.as_path()] {
            let dir = base.join(name);
            match std::fs::create_dir_all(&dir).and_then(|()| File::create(dir.join(MAIN_FILE))) {
                Ok(file) => {
                    return Some(Self {
                        dir,
                        out: Some(BufWriter::new(file)),
                        notes,
                    });
                }
                Err(error) => {
                    notes.push(format!("could not write under {}: {error}", base.display()))
                }
            }
        }
        None
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    fn note(&mut self, note: String) {
        if self.notes.len() < MAX_NOTES {
            self.notes.push(note);
        }
    }

    pub(crate) fn text(&mut self, text: &str) {
        let written = match self.out.as_mut() {
            Some(out) => out.write_all(text.as_bytes()),
            None => Ok(()),
        };
        if let Err(error) = written {
            self.out = None;
            self.note(format!("{MAIN_FILE} stopped taking writes: {error}"));
        }
    }

    pub(crate) fn section(&mut self, title: &str, body: impl FnOnce(&mut String)) {
        let mut text = String::new();
        let outcome = catch_unwind(AssertUnwindSafe(|| body(&mut text)));
        self.text(&format!("\n== {title} ==\n"));
        if text.len() > MAX_SECTION_BYTES {
            let mut end = MAX_SECTION_BYTES;
            while end > 0 && !text.is_char_boundary(end) {
                end = end.saturating_sub(1);
            }
            text.truncate(end);
            let _ = write!(text, "\n(cut at {MAX_SECTION_BYTES} bytes)");
            self.note(format!(
                "section {title:?} was cut at {MAX_SECTION_BYTES} bytes"
            ));
        }
        self.text(&text);
        if !text.ends_with('\n') {
            self.text("\n");
        }
        if let Err(payload) = outcome {
            super::take_panic();
            let why = payload_text(payload.as_ref());
            self.text(&format!("FAILED: this step panicked: {why}\n"));
            self.note(format!("section {title:?} panicked: {why}"));
        }
    }

    pub(crate) fn file(
        &mut self,
        name: &str,
        write: impl FnOnce(&mut dyn Write) -> io::Result<String>,
    ) -> String {
        let path = self.dir.join(name);
        let outcome = catch_unwind(AssertUnwindSafe(|| -> io::Result<String> {
            let mut out = BufWriter::new(File::create(&path)?);
            let summary = write(&mut out)?;
            out.flush()?;
            Ok(summary)
        }));
        match outcome {
            Ok(Ok(summary)) => summary,
            Ok(Err(error)) => {
                self.note(format!("{name} is incomplete: {error}"));
                format!("FAILED: {name} is incomplete: {error}")
            }
            Err(payload) => {
                super::take_panic();
                let why = payload_text(payload.as_ref());
                self.note(format!("{name} is incomplete: the step panicked: {why}"));
                format!("FAILED: {name} is incomplete: the step panicked: {why}")
            }
        }
    }

    pub(crate) fn finish(mut self) -> PathBuf {
        let notes = std::mem::take(&mut self.notes);
        self.text("\n== dump notes ==\n");
        if notes.is_empty() {
            self.text("every step completed\n");
        }
        for note in &notes {
            self.text(&format!("{note}\n"));
        }
        if let Some(out) = self.out.as_mut() {
            let _ = out.flush();
        }
        self.dir
    }
}

pub(crate) fn payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        String::from("(a payload that is not text)")
    }
}

pub(crate) fn utc_stamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    stamp_of(seconds)
}

pub(crate) fn stamp_of(seconds: u64) -> String {
    let days = seconds / 86_400;
    let of_day = seconds % 86_400;
    let shifted = days.saturating_add(719_468);
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        of_day / 3_600,
        of_day % 3_600 / 60,
        of_day % 60
    )
}
