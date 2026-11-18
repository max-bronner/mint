use std::cell::RefCell;
use std::time::{Duration, Instant};

pub(crate) const STAGES: [&str; 19] = [
    "update",
    "frame",
    "hud",
    "view",
    "shroud",
    "scene",
    "poses",
    "sync",
    "units",
    "visuals",
    "particles",
    "shadow",
    "water",
    "mirror",
    "render",
    "present",
    "post",
    "gpu wait",
    "snapshot",
];

macro_rules! stage {
    ($name:literal) => {
        const {
            let index = crate::perf::position(&crate::render_perf::STAGES, $name);
            assert!(
                index < crate::render_perf::STAGES.len(),
                "no such render stage"
            );
            index
        }
    };
}
pub(crate) use stage;

const COMPOSE: std::ops::RangeInclusive<usize> = stage!("view")..=stage!("render");

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FrameCounts {
    pub instances: usize,
    pub drawn: u32,
    pub culled: u32,
    pub draw_calls: u32,
    pub shadow_drawn: u32,
    pub shadow_draw_calls: u32,
    pub unit_drawables: usize,
    pub unit_sampled: usize,
    pub unit_out_of_view: usize,
    pub unit_reused: usize,
    pub unit_instances: usize,
    pub skinned_vertices: usize,
    pub uploaded_bytes: usize,
    pub unit_sample_ms: f64,
    pub unit_skin_ms: f64,
    pub unit_place_ms: f64,
    pub unit_pair_ms: f64,
    pub unit_pose_threads: usize,
    pub unit_pose_cpu_ms: f64,
    pub unit_pose_worst_ms: f64,
    pub unit_pose_sample_ms: f64,
    pub unit_state_entries: usize,
    pub mirror: crate::reflection::MirrorCounts,
    pub submits: (u32, u32),
    pub passes: u32,
}

const PERIOD: usize = 150;

#[derive(Default)]
struct Samples {
    stages: Vec<[f64; STAGES.len()]>,
    counts: Vec<FrameCounts>,
    window: usize,
    printed: usize,
    keep_run: bool,
}

thread_local! {
    static SAMPLES: RefCell<Samples> = RefCell::new(Samples::default());
    static CURRENT: RefCell<[f64; STAGES.len()]> = const { RefCell::new([0.0; STAGES.len()]) };
    static LAST: RefCell<[f64; STAGES.len()]> = const { RefCell::new([0.0; STAGES.len()]) };
    static LAST_COUNTS: RefCell<FrameCounts> = RefCell::new(FrameCounts::default());
}

pub(crate) struct Clock {
    last: Option<Instant>,
}

impl Clock {
    pub(crate) fn start() -> Self {
        Self {
            last: crate::perf::enabled().then(Instant::now),
        }
    }

    pub(crate) fn lap(&mut self, stage: usize) {
        let Some(last) = self.last else { return };
        let now = Instant::now();
        add_current(stage, (now - last).as_secs_f64() * 1000.0);
        self.last = Some(now);
    }

    pub(crate) fn lap_less(&mut self, stage: usize, other: usize, part: Duration) {
        let Some(last) = self.last else { return };
        let now = Instant::now();
        let whole = (now - last).as_secs_f64() * 1000.0;
        let part = part.as_secs_f64() * 1000.0;
        add_current(stage, whole - part);
        add_current(other, part);
        self.last = Some(now);
    }
}

pub(crate) fn add_current(stage: usize, ms: f64) {
    if !crate::perf::enabled() {
        return;
    }
    CURRENT.with(|current| {
        if let Some(slot) = current.borrow_mut().get_mut(stage) {
            *slot += ms;
        }
    });
}

pub(crate) fn time<T>(stage: usize, work: impl FnOnce() -> T) -> T {
    if !crate::perf::enabled() {
        return work();
    }
    let started = Instant::now();
    let result = work();
    add_current(stage, started.elapsed().as_secs_f64() * 1000.0);
    result
}

pub(crate) fn keep_run() {
    SAMPLES.with(|samples| samples.borrow_mut().keep_run = true);
}

pub(crate) fn finish_frame(counts: FrameCounts) {
    if !crate::perf::enabled() {
        return;
    }
    let stages = CURRENT.with(|current| std::mem::take(&mut *current.borrow_mut()));
    LAST.with(|last| *last.borrow_mut() = stages);
    LAST_COUNTS.with(|last| *last.borrow_mut() = counts);
    SAMPLES.with(|samples| {
        let mut samples = samples.borrow_mut();
        if samples.stages.len().saturating_sub(samples.window) >= PERIOD {
            let window = samples.window;
            print_table(
                samples.stages.get(window..).unwrap_or_default(),
                samples.counts.get(window..).unwrap_or_default(),
                samples.printed.saturating_add(window).saturating_add(1),
                0,
            );
            if samples.keep_run {
                samples.window = samples.stages.len();
            } else {
                samples.printed = samples.printed.saturating_add(samples.stages.len());
                samples.stages.clear();
                samples.counts.clear();
                samples.window = 0;
            }
        }
        samples.stages.push(stages);
        samples.counts.push(counts);
    });
}

pub(crate) fn add_to_last(stage: usize, ms: f64) {
    LAST.with(|last| {
        if let Some(slot) = last.borrow_mut().get_mut(stage) {
            *slot += ms;
        }
    });
    SAMPLES.with(|samples| {
        if let Some(slot) = samples
            .borrow_mut()
            .stages
            .last_mut()
            .and_then(|frame| frame.get_mut(stage))
        {
            *slot += ms;
        }
    });
}

pub(crate) fn last_frame_stages(stage: &mut dyn FnMut(&'static str, f64)) {
    if !crate::perf::enabled() {
        return;
    }
    LAST.with(|last| {
        for (name, ms) in STAGES.iter().zip(last.borrow().iter()) {
            stage(name, *ms);
        }
    });
    LAST_COUNTS.with(|last| {
        let counts = *last.borrow();
        stage("mirror_rendered_count", f64::from(counts.mirror.rendered));
        stage("mirror_skipped_count", f64::from(counts.mirror.skipped));
        stage(
            "draw_call_count",
            f64::from(
                counts
                    .draw_calls
                    .saturating_add(counts.shadow_draw_calls)
                    .saturating_add(counts.mirror.draw_calls),
            ),
        );
        stage("submit_count", f64::from(counts.submits.0));
        stage("pass_count", f64::from(counts.passes));
    });
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0u32), |(s, n), v| (s + v, n.saturating_add(1)));
    if n == 0 {
        0.0
    } else {
        sum / f64::from(n)
    }
}

pub(crate) fn report(warmup: usize) {
    if !crate::perf::enabled() {
        return;
    }
    SAMPLES.with(|samples| {
        let samples = samples.borrow();
        let window = samples.window;
        print_table(
            samples.stages.get(window..).unwrap_or_default(),
            samples.counts.get(window..).unwrap_or_default(),
            samples.printed.saturating_add(window).saturating_add(1),
            warmup,
        );
    });
}

pub(crate) fn report_run() {
    if !crate::perf::enabled() {
        return;
    }
    SAMPLES.with(|samples| {
        let samples = samples.borrow();
        println!("render perf: whole run");
        println!(
            "render perf: snapshot read stale {} time(s)",
            crate::snapshot::fallbacks()
        );
        print_table(
            &samples.stages,
            &samples.counts,
            samples.printed.saturating_add(1),
            0,
        );
    });
}

fn print_table(
    stages: &[[f64; STAGES.len()]],
    counts: &[FrameCounts],
    first: usize,
    warmup: usize,
) {
    let frames: Vec<&[f64; STAGES.len()]> = stages.iter().skip(warmup).collect();
    if frames.is_empty() {
        return;
    }
    println!(
        "render perf: frame {}-{}, {} sampled after {warmup} warm-up, ms",
        first.saturating_add(warmup),
        first.saturating_add(stages.len()).saturating_sub(1),
        frames.len()
    );
    println!("render perf: stage          mean_ms      p99_ms      max_ms");
    let mut cpu_per_frame = vec![0.0; frames.len()];
    let mut compose_per_frame = vec![0.0; frames.len()];
    let waits = [stage!("present"), stage!("gpu wait"), stage!("snapshot")];
    for (index, name) in STAGES.iter().enumerate() {
        let values: Vec<f64> = frames
            .iter()
            .map(|f| f.get(index).copied().unwrap_or(0.0))
            .collect();
        if !waits.contains(&index) {
            for (total, ms) in cpu_per_frame.iter_mut().zip(values.iter()) {
                *total += *ms;
            }
        }
        if COMPOSE.contains(&index) {
            for (total, ms) in compose_per_frame.iter_mut().zip(values.iter()) {
                *total += *ms;
            }
        }
        let (m, p99, max) = crate::perf::stats(&values);
        println!("render perf: {name:<14} {m:>9.3} {p99:>11.3} {max:>11.3}");
    }
    let (m, p99, max) = crate::perf::stats(&compose_per_frame);
    println!(
        "render perf: {:<14} {m:>9.3} {p99:>11.3} {max:>11.3}",
        "compose"
    );
    let (m, p99, max) = crate::perf::stats(&cpu_per_frame);
    println!(
        "render perf: {:<14} {m:>9.3} {p99:>11.3} {max:>11.3}",
        "CPU TOTAL"
    );
    let counts: Vec<&FrameCounts> = counts.iter().skip(warmup).collect();
    #[allow(clippy::cast_precision_loss)]
    let avg = |f: &dyn Fn(&FrameCounts) -> f64| mean(counts.iter().map(|c| f(c)));
    println!(
        "render perf: instances {:.0} submitted, {:.0} drawn, {:.0} culled, {:.0} draw call(s); \
         shadow {:.0} drawn in {:.0} call(s)",
        avg(&|c| c.instances as f64),
        avg(&|c| f64::from(c.drawn)),
        avg(&|c| f64::from(c.culled)),
        avg(&|c| f64::from(c.draw_calls)),
        avg(&|c| f64::from(c.shadow_drawn)),
        avg(&|c| f64::from(c.shadow_draw_calls)),
    );
    println!(
        "render perf: units {:.0} drawable(s), {:.0} posed, {:.0} kept last frame's pose, \
         {:.0} out of every view, {:.0} instance(s), {:.0} vertices skinned on the GPU, \
         {:.1} MB bone palette; pose {:.3} ms, palette {:.3} ms, emit {:.3} ms",
        avg(&|c| c.unit_drawables as f64),
        avg(&|c| c.unit_sampled as f64),
        avg(&|c| c.unit_reused as f64),
        avg(&|c| c.unit_out_of_view as f64),
        avg(&|c| c.unit_instances as f64),
        avg(&|c| c.skinned_vertices as f64),
        avg(&|c| c.uploaded_bytes as f64) / 1.0e6,
        avg(&|c| c.unit_sample_ms),
        avg(&|c| c.unit_skin_ms),
        avg(&|c| c.unit_place_ms),
    );
    println!(
        "render perf: pairing {:.3} ms, {:.0} thread(s); pose {:.3} ms over all of them, \
         dearest drawable {:.3} ms, sampling {:.3} ms of it, {:.0} state entry/entries",
        avg(&|c| c.unit_pair_ms),
        avg(&|c| c.unit_pose_threads as f64),
        avg(&|c| c.unit_pose_cpu_ms),
        avg(&|c| c.unit_pose_worst_ms),
        avg(&|c| c.unit_pose_sample_ms),
        avg(&|c| c.unit_state_entries as f64),
    );
    println!(
        "render perf: mirror {:.2} pass(es) rendered, {:.2} skipped, {:.0} draw call(s)",
        avg(&|c| f64::from(c.mirror.rendered)),
        avg(&|c| f64::from(c.mirror.skipped)),
        avg(&|c| f64::from(c.mirror.draw_calls)),
    );
    println!(
        "render perf: {:.2} pass(es) recorded, {:.2} queue submit(s) a frame, {:.2} forced by a \
         texture update",
        avg(&|c| f64::from(c.passes)),
        avg(&|c| f64::from(c.submits.0)),
        avg(&|c| f64::from(c.submits.1)),
    );
}
