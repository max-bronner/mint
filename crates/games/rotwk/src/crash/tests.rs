use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use super::ring::{Ring, MAX_LINE_BYTES};
use super::*;

const SECTIONS: [&str; 11] = [
    "failure",
    "build",
    "launch",
    "match setup",
    "simulation",
    "camera and selection",
    "replay",
    "recent presentation events",
    "recent frames",
    "recent log lines",
    "dump notes",
];

struct Root(PathBuf);

impl Root {
    fn new(test: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("mint-crash-test-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }

    fn only_dump(&self) -> String {
        let dumps: Vec<PathBuf> = std::fs::read_dir(&self.0)
            .expect("the dump root exists")
            .map(|entry| entry.expect("a readable entry").path())
            .collect();
        assert_eq!(dumps.len(), 1, "one dump: {dumps:?}");
        std::fs::read_to_string(dumps[0].join(MAIN_FILE)).expect("the dump has its main file")
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct FakeMatch {
    view_panics: bool,
}

impl MatchReport for FakeMatch {
    fn setup(&self, out: &mut String) {
        out.push_str("seed: 0x2a");
    }

    fn simulation(&self, out: &mut String) {
        out.push_str("tick: 7");
    }

    fn view(&self, out: &mut String) {
        out.push_str("camera eye: 1, 2, 3");
        assert!(!self.view_panics, "the camera is gone");
    }

    fn replay(&self, out: &mut dyn io::Write) -> io::Result<String> {
        writeln!(out, "c 3 0 0 Stop")?;
        Ok(String::from("1 command(s)"))
    }
}

#[derive(Default)]
struct FakeApp {
    running: FakeMatch,
    update_panics: bool,
    survived: Option<Failure>,
}

impl Subject for FakeApp {
    fn running_match(&self) -> Option<&dyn MatchReport> {
        Some(&self.running)
    }

    fn take_failure(&mut self) -> Option<Failure> {
        self.survived.take()
    }
}

impl App for FakeApp {
    fn input(&mut self, _event: &InputEvent) {}

    fn update(&mut self, _step: u64) {
        assert!(!self.update_panics, "the update fell over");
    }

    fn draw(&mut self, _renderer: &mut Renderer, _blend: f32) -> mint_render::Result<()> {
        Ok(())
    }
}

fn section_titles(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("== ")?.strip_suffix(" =="))
        .collect()
}

#[test]
fn a_full_ring_keeps_only_its_newest_lines() {
    let mut ring = Ring::new(4);
    for line in 0..10 {
        ring.push(format_args!("line {line}"));
    }
    assert_eq!(
        ring.lines().collect::<Vec<_>>(),
        ["line 6", "line 7", "line 8", "line 9"]
    );
    assert_eq!(ring.dropped(), 6);
}

#[test]
fn a_long_line_is_cut_at_the_bound_on_a_character_boundary() {
    let mut ring = Ring::new(1);
    ring.push(format_args!("{}", "é".repeat(MAX_LINE_BYTES)));
    let kept = ring.lines().next().expect("one line");
    assert_eq!(kept.len(), MAX_LINE_BYTES);
    assert!(kept.chars().all(|c| c == 'é'));
}

#[test]
fn a_stamp_is_the_utc_calendar_date_across_a_leap_day() {
    assert_eq!(dump::stamp_of(951_782_400 + 3_723), "20000229-010203");
}

#[test]
fn a_dump_with_no_match_still_carries_every_section() {
    let root = Root::new("no-match");
    write(&root.0, &Failure::message("loading", "no map"), None).expect("a dump");
    let text = root.only_dump();
    assert_eq!(section_titles(&text), SECTIONS);
}

#[test]
fn a_step_that_panics_is_noted_and_the_later_sections_still_land() {
    let root = Root::new("step-panics");
    let running = FakeMatch { view_panics: true };
    let dir = write(&root.0, &Failure::message("draw", "x"), Some(&running)).expect("a dump");
    let text = root.only_dump();
    assert!(
        text.contains("FAILED: this step panicked: the camera is gone"),
        "{text}"
    );
    assert!(text.contains("== recent log lines =="), "{text}");
    assert!(
        text.contains("section \"camera and selection\" panicked"),
        "{text}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(REPLAY_FILE)).expect("the replay file"),
        "c 3 0 0 Stop\n"
    );
}

#[test]
fn a_draw_error_from_the_frame_loop_writes_a_dump_with_its_whole_chain() {
    let root = Root::new("draw-error");
    let mut app = Dumping::new(FakeApp::default(), &root.0);
    app.update(0);
    let failed: mint_render::Result<()> =
        Err(mint_render::Error::UnknownHandle.about("particle system \"Smoke\" (slot 3)"));
    assert!(app.settle("draw", failed).is_err());
    let text = root.only_dump();
    assert_eq!(section_titles(&text), SECTIONS, "{text}");
    assert!(
        text.contains("error: particle system \"Smoke\" (slot 3)\n  caused by: "),
        "{text}"
    );
    assert!(
        text.contains("tick: 7") && text.contains("seed: 0x2a"),
        "{text}"
    );
}

#[test]
fn a_panic_in_the_update_writes_a_dump_and_keeps_unwinding() {
    let root = Root::new("update-panic");
    let mut app = Dumping::new(
        FakeApp {
            update_panics: true,
            ..FakeApp::default()
        },
        &root.0,
    );
    let outcome = catch_unwind(AssertUnwindSafe(|| app.update(0)));
    assert!(outcome.is_err(), "the panic goes on to the caller");
    let text = root.only_dump();
    assert!(
        text.contains("kind: panic\nstage: update\nmessage: the update fell over"),
        "{text}"
    );
}

#[test]
fn a_failure_the_app_survived_is_dumped_after_the_update() {
    let root = Root::new("survived");
    let mut app = Dumping::new(
        FakeApp {
            survived: Some(Failure::message("simulation", "the tick refused")),
            ..FakeApp::default()
        },
        &root.0,
    );
    app.update(0);
    let text = root.only_dump();
    assert!(
        text.contains("stage: simulation\nerror: the tick refused"),
        "{text}"
    );
}
