mod dump;
mod ring;

use std::cell::Cell;
use std::fmt::{self, Write as _};
use std::io;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, TryLockError};

use mint_app::{App, CursorRequest, InputEvent};
use mint_render::Renderer;

use dump::Report;
pub(crate) use dump::{MAIN_FILE, REPLAY_FILE};
use ring::Ring;

const LOG_LINES: usize = 512;
const EVENT_LINES: usize = 512;
const FRAME_LINES: usize = 180;
const MAX_CHAIN: usize = 32;
const MAX_BACKTRACE_BYTES: usize = 64 * 1024;
const MAX_ENV_VARS: usize = 64;
const MAX_DUMPS: u32 = 3;

static LOGS: Mutex<Vec<(String, Arc<Mutex<Ring>>)>> = Mutex::new(Vec::new());
const MAX_LOG_THREADS: usize = 16;
static EVENTS: Mutex<Ring> = Mutex::new(Ring::new(EVENT_LINES));
static FRAMES: Mutex<Ring> = Mutex::new(Ring::new(FRAME_LINES));
static TICK: AtomicU64 = AtomicU64::new(0);
static DRAWN: AtomicU64 = AtomicU64::new(0);
static WRITTEN: AtomicU32 = AtomicU32::new(0);
static SEQUENCE: AtomicU32 = AtomicU32::new(0);
static PANIC: Mutex<Option<PanicRecord>> = Mutex::new(None);
static ROOT: OnceLock<PathBuf> = OnceLock::new();

thread_local! {
    static LOG: Option<Arc<Mutex<Ring>>> = register_log();

    static GUARDED: Cell<u32> = const { Cell::new(0) };
}

fn push(ring: &Mutex<Ring>, line: fmt::Arguments<'_>) {
    let tick = TICK.load(Ordering::Relaxed);
    let drawn = DRAWN.load(Ordering::Relaxed);
    ring.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(format_args!("t{tick} f{drawn} {line}"));
}

fn register_log() -> Option<Arc<Mutex<Ring>>> {
    let mut logs = LOGS.lock().unwrap_or_else(PoisonError::into_inner);
    if logs.len() >= MAX_LOG_THREADS {
        return None;
    }
    let ring = Arc::new(Mutex::new(Ring::new(LOG_LINES)));
    let thread = std::thread::current();
    logs.push((
        thread.name().unwrap_or("unnamed").to_owned(),
        Arc::clone(&ring),
    ));
    Some(ring)
}

pub(crate) fn log(line: fmt::Arguments<'_>) {
    let _ = LOG.try_with(|ring| {
        if let Some(ring) = ring {
            push(ring, line);
        }
    });
}

pub(crate) fn event(line: fmt::Arguments<'_>) {
    push(&EVENTS, line);
}

pub(crate) fn frame(line: fmt::Arguments<'_>) {
    push(&FRAMES, line);
    DRAWN.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn set_tick(tick: u64) {
    TICK.store(tick, Ordering::Relaxed);
}

pub(crate) fn current() -> (u64, u64) {
    (DRAWN.load(Ordering::Relaxed), TICK.load(Ordering::Relaxed))
}

pub(crate) fn written() -> u32 {
    WRITTEN.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    Error {
        stage: &'static str,
        chain: Vec<String>,
    },
    Panic {
        stage: &'static str,
        record: PanicRecord,
    },
}

impl Failure {
    pub(crate) fn error(stage: &'static str, error: &(dyn std::error::Error + 'static)) -> Self {
        let mut chain = vec![error.to_string()];
        let mut cause = error.source();
        while let Some(source) = cause {
            if chain.len() >= MAX_CHAIN {
                chain.push(String::from("(chain cut here)"));
                break;
            }
            chain.push(source.to_string());
            cause = source.source();
        }
        Self::Error { stage, chain }
    }

    pub(crate) fn message(stage: &'static str, message: &str) -> Self {
        Self::Error {
            stage,
            chain: vec![message.to_owned()],
        }
    }

    fn describe(&self, out: &mut String) {
        match self {
            Self::Error { stage, chain } => {
                let _ = writeln!(out, "kind: error\nstage: {stage}");
                for (depth, link) in chain.iter().enumerate() {
                    let lead = if depth == 0 { "error" } else { "  caused by" };
                    let _ = writeln!(out, "{lead}: {link}");
                }
            }
            Self::Panic { stage, record } => {
                let _ = writeln!(
                    out,
                    "kind: panic\nstage: {stage}\nmessage: {}\nlocation: {}\nthread: {}\nbacktrace:\n{}",
                    record.message, record.location, record.thread, record.backtrace
                );
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PanicRecord {
    message: String,
    location: String,
    thread: String,
    backtrace: String,
}

impl PanicRecord {
    fn capture(info: &std::panic::PanicHookInfo<'_>) -> Self {
        let mut backtrace = std::backtrace::Backtrace::force_capture().to_string();
        cut(&mut backtrace, MAX_BACKTRACE_BYTES);
        Self {
            message: dump::payload_text(info.payload()),
            location: info
                .location()
                .map_or_else(|| String::from("unknown"), ToString::to_string),
            thread: std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_owned(),
            backtrace,
        }
    }

    fn of_payload(payload: &(dyn std::any::Any + Send)) -> Self {
        Self {
            message: dump::payload_text(payload),
            location: String::from("unknown (the panic hook was not installed)"),
            thread: String::from("unknown"),
            backtrace: String::from("none"),
        }
    }
}

fn cut(text: &mut String, max: usize) {
    if text.len() > max {
        let mut end = max;
        while end > 0 && !text.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        text.truncate(end);
        text.push_str("\n(cut)");
    }
}

fn take_panic() -> Option<PanicRecord> {
    PANIC.lock().unwrap_or_else(PoisonError::into_inner).take()
}

pub(crate) trait MatchReport {
    fn setup(&self, out: &mut String);
    fn simulation(&self, out: &mut String);
    fn view(&self, out: &mut String);
    fn replay(&self, out: &mut dyn io::Write) -> io::Result<String>;
}

pub(crate) trait Subject {
    fn running_match(&self) -> Option<&dyn MatchReport>;
    fn take_failure(&mut self) -> Option<Failure> {
        None
    }
}

fn default_root() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    if let Some(dir) = var("MINT_CRASH_DIR") {
        return PathBuf::from(dir);
    }
    let base = var("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| var("XDG_STATE_HOME").map(PathBuf::from))
        .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".local").join("state")));
    match base {
        Some(base) => base.join("mint").join("crash-dumps"),
        None => std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir)
            .join("crash-dumps"),
    }
}

pub(crate) fn root() -> &'static Path {
    ROOT.get_or_init(default_root)
}

pub(crate) fn install() {
    let root = root();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        let record = PanicRecord::capture(info);
        let guarded = GUARDED.try_with(|depth| depth.get() > 0).unwrap_or(false);
        if guarded {
            if let Ok(mut slot) = PANIC.try_lock() {
                *slot = Some(record);
            }
        } else if WRITTEN.load(Ordering::Relaxed) < MAX_DUMPS {
            let failure = Failure::Panic {
                stage: "outside the frame loop",
                record,
            };
            announce(write(root, &failure, None));
        }
    }));
}

pub(crate) fn report_error(stage: &'static str, error: &(dyn std::error::Error + 'static)) {
    announce(write(root(), &Failure::error(stage, error), None));
}

pub(crate) fn report_message(stage: &'static str, message: &str) {
    announce(write(root(), &Failure::message(stage, message), None));
}

fn announce(dir: Option<PathBuf>) {
    match dir {
        Some(dir) => eprintln!(
            "crash dump written: {}\n  send that whole folder with the bug report; it names local paths, so read \
             it before posting it publicly",
            dir.display()
        ),
        None => eprintln!("crash dump: could not be written anywhere"),
    }
}

pub(crate) fn write(
    root: &Path,
    failure: &Failure,
    subject: Option<&dyn MatchReport>,
) -> Option<PathBuf> {
    let _scope = Scope::enter();
    let name = format!(
        "{}-p{}-{}",
        dump::utc_stamp(),
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let mut report = Report::create(root, &name)?;
    WRITTEN.fetch_add(1, Ordering::Relaxed);
    report.text(&format!(
        "mint-rotwk crash dump\n\
         NOTE: this dump names paths on the machine that wrote it (the game's install folders, the \
         user's own folders). Read it before posting it anywhere public.\n\
         written: {} UTC\nfolder: {}\nfiles: {MAIN_FILE}, {REPLAY_FILE} (when a match was running)\n",
        dump::utc_stamp(),
        report.dir().display()
    ));
    report.section("failure", |out| failure.describe(out));
    report.section("build", build);
    report.section("launch", launch);
    match subject {
        Some(subject) => {
            report.section("match setup", |out| subject.setup(out));
            report.section("simulation", |out| subject.simulation(out));
            report.section("camera and selection", |out| subject.view(out));
            let summary = report.file(REPLAY_FILE, |out| subject.replay(out));
            report.section("replay", |out| out.push_str(&summary));
        }
        None => {
            for title in [
                "match setup",
                "simulation",
                "camera and selection",
                "replay",
            ] {
                report.section(title, |out| {
                    out.push_str("not available: no match was reachable when this was written");
                });
            }
        }
    }
    report.section("recent presentation events", |out| drain(&EVENTS, out));
    report.section("recent frames", |out| drain(&FRAMES, out));
    report.section("recent log lines", drain_logs);
    Some(report.finish())
}

fn build(out: &mut String) {
    let _ = writeln!(
        out,
        "commit: {}\ntree when this crate was last built: {}\nprofile: {}\ndebug assertions: {}\n\
         version: {}\ntarget: {} {}",
        env!("MINT_BUILD_COMMIT"),
        env!("MINT_BUILD_TREE"),
        env!("MINT_BUILD_PROFILE"),
        cfg!(debug_assertions),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}

fn launch(out: &mut String) {
    out.push_str("command line:");
    for argument in std::env::args_os() {
        let _ = write!(out, " {:?}", argument.to_string_lossy());
    }
    let _ = writeln!(
        out,
        "\nworking directory: {}",
        std::env::current_dir().map_or_else(
            |e| format!("unknown ({e})"),
            |dir| dir.display().to_string()
        )
    );
    for (name, value) in std::env::vars_os()
        .filter(|(name, _)| name.to_string_lossy().starts_with("MINT_"))
        .take(MAX_ENV_VARS)
    {
        let _ = writeln!(
            out,
            "{}={}",
            name.to_string_lossy(),
            value.to_string_lossy()
        );
    }
}

fn drain_logs(out: &mut String) {
    let logs = match LOGS.try_lock() {
        Ok(logs) => logs,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            out.push_str("not available: a thread was registering its buffer when this was read");
            return;
        }
    };
    if logs.is_empty() {
        out.push_str(
            "no line was kept
",
        );
    }
    for (thread, ring) in logs.iter() {
        let _ = writeln!(out, "thread {thread}:");
        drain(ring, out);
    }
}

fn drain(ring: &Mutex<Ring>, out: &mut String) {
    let ring = match ring.try_lock() {
        Ok(ring) => ring,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            out.push_str("not available: the buffer was being written when this was read");
            return;
        }
    };
    let _ = writeln!(
        out,
        "(tT = simulation tick, fF = drawn frame; {} older line(s) overwritten)",
        ring.dropped()
    );
    for line in ring.lines() {
        out.push_str(line);
        out.push('\n');
    }
}

struct Scope;

impl Scope {
    fn enter() -> Self {
        let _ = GUARDED.try_with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let _ = GUARDED.try_with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

pub(crate) struct Dumping<A> {
    app: A,
    root: PathBuf,
    written: u32,
}

impl<A: App + Subject> Dumping<A> {
    pub(crate) fn new(app: A, root: &Path) -> Self {
        Self {
            app,
            root: root.to_path_buf(),
            written: 0,
        }
    }

    pub(crate) fn app(&self) -> &A {
        &self.app
    }

    pub(crate) fn app_mut(&mut self) -> &mut A {
        &mut self.app
    }

    fn dump(&mut self, failure: &Failure) {
        if self.written >= MAX_DUMPS {
            return;
        }
        self.written = self.written.saturating_add(1);
        announce(write(&self.root, failure, self.app.running_match()));
    }

    fn guarded<T>(&mut self, stage: &'static str, run: impl FnOnce(&mut A) -> T) -> T {
        let outcome = {
            let _scope = Scope::enter();
            catch_unwind(AssertUnwindSafe(|| run(&mut self.app)))
        };
        match outcome {
            Ok(value) => value,
            Err(payload) => {
                let record =
                    take_panic().unwrap_or_else(|| PanicRecord::of_payload(payload.as_ref()));
                self.dump(&Failure::Panic { stage, record });
                resume_unwind(payload)
            }
        }
    }

    pub(crate) fn settle<T>(
        &mut self,
        stage: &'static str,
        result: mint_render::Result<T>,
    ) -> mint_render::Result<T> {
        if let Err(error) = &result {
            self.dump(&Failure::error(stage, error));
        }
        result
    }
}

impl<A: App + Subject> App for Dumping<A> {
    fn input(&mut self, event: &InputEvent) {
        self.guarded("input", |app| app.input(event));
    }

    fn update(&mut self, step: u64) {
        self.guarded("update", |app| app.update(step));
        if let Some(failure) = self.app.take_failure() {
            self.dump(&failure);
        }
    }

    fn frame(&mut self, dt: f32) {
        self.guarded("frame", |app| app.frame(dt));
    }

    fn draw(&mut self, renderer: &mut Renderer, blend: f32) -> mint_render::Result<()> {
        let drawn = self.guarded("draw", |app| app.draw(renderer, blend));
        self.settle("draw", drawn)
    }

    fn cursor(&mut self) -> CursorRequest {
        self.guarded("cursor", App::cursor)
    }

    fn speed(&self) -> f64 {
        self.app.speed()
    }

    fn frame_stages(&self, stage: &mut dyn FnMut(&'static str, f64)) {
        self.app.frame_stages(stage);
    }

    fn paused(&self) -> bool {
        self.app.paused()
    }

    fn exit_requested(&self) -> bool {
        self.app.exit_requested()
    }

    fn abandon_on_exit(&self) -> bool {
        self.app.abandon_on_exit()
    }
}

#[cfg(test)]
mod tests;
