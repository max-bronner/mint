use std::fs::{self, File};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

struct Capture {
    file: File,
    path: PathBuf,
    lines: u32,
    marks: u32,
}

static CAPTURING: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<Option<Capture>> = Mutex::new(None);

pub(crate) fn is_capturing() -> bool {
    CAPTURING.load(Ordering::Relaxed)
}

pub(crate) fn toggle() {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(capture) = state.take() {
        CAPTURING.store(false, Ordering::Release);
        println!(
            "capture: off - {} lines -> {}",
            capture.lines,
            capture.path.display()
        );
        return;
    }
    match start() {
        Ok(capture) => {
            println!("capture: on -> {}", capture.path.display());
            *state = Some(capture);
            CAPTURING.store(true, Ordering::Release);
        }
        Err(message) => println!("capture: could not start - {message}"),
    }
}

fn start() -> Result<Capture, String> {
    let dir = PathBuf::from("captures");
    fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let name = format!("capture-{}.log", timestamp());
    let path = dir.join(name);
    let file = File::create(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let absolute = fs::canonicalize(&path).unwrap_or(path);
    Ok(Capture {
        file,
        path: absolute,
        lines: 0,
        marks: 0,
    })
}

pub(crate) fn write(text: &str) {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(capture) = state.as_mut() else {
        return;
    };
    append(capture, text);
}

pub(crate) fn mark() {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(capture) = state.as_mut() else {
        return;
    };
    capture.marks += 1;
    let text = format!("mark {}", capture.marks);
    append(capture, &text);
    println!("{text}");
}

pub(crate) fn snapshot_line(text: &str) {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(capture) = state.as_mut() else {
        return;
    };
    append(capture, text);
}

fn append(capture: &mut Capture, text: &str) {
    let (frame, tick) = crate::crash::current();
    if writeln!(capture.file, "[f{frame} t{tick}] {text}").is_ok() {
        capture.lines += 1;
        let _ = capture.file.flush();
    }
}

fn timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(i64::try_from(seconds / 86400).unwrap_or(0));
    let of_day = seconds % 86400;
    let (hh, mm, ss) = (of_day / 3600, (of_day / 60) % 60, of_day % 60);
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    (year, m as u32, d as u32)
}

#[cfg(test)]
pub(crate) fn lines_for_test() -> u32 {
    STATE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map_or(0, |capture| capture.lines)
}

#[cfg(test)]
pub(crate) fn read_for_test() -> String {
    let path = STATE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|capture| capture.path.clone());
    path.and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::civil_from_days;

    #[test]
    fn the_epoch_is_1970_01_01() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn a_known_date_round_trips() {
        assert_eq!(civil_from_days(19782), (2024, 2, 29));
    }

    #[test]
    fn a_capture_admits_every_route_numbers_its_marks_and_stops_cleanly() {
        assert!(!super::is_capturing());
        super::toggle();
        assert!(super::is_capturing());

        let before = super::lines_for_test();
        crate::diag::emit(
            crate::diag::Route::Skip,
            format_args!("wall: a line the console filter would have skipped"),
        );
        assert_eq!(super::lines_for_test(), before + 1);

        super::mark();
        super::mark();
        let text = super::read_for_test();
        assert!(text.contains("] mark 1"));
        assert!(text.contains("] mark 2"));

        super::toggle();
        assert!(!super::is_capturing());
        assert_eq!(super::lines_for_test(), 0);
        crate::diag::emit(
            crate::diag::Route::Skip,
            format_args!("wall: after the stop"),
        );
        assert_eq!(super::lines_for_test(), 0);
    }
}
