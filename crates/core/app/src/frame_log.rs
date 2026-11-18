use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

use crate::App;

const FLUSH_EVERY: u64 = 256;

const OWN: [&str; 4] = ["update", "frame", "draw", "present"];

const COUNT: &str = "_count";

#[derive(Debug, Default)]
pub struct FrameLog {
    out: Option<BufWriter<File>>,
    frames: u64,
    stages: Vec<(&'static str, f64)>,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameMarks {
    pub start: Instant,
    pub stepped: Instant,
    pub framed: Instant,
    pub drawn: Instant,
}

impl FrameLog {
    #[must_use]
    pub fn from_env() -> Self {
        let out = std::env::var_os("MINT_FRAME_LOG")
            .and_then(|path| File::create(path).ok())
            .map(BufWriter::new);
        Self {
            out,
            frames: 0,
            stages: Vec::new(),
        }
    }

    pub fn record<A: App + ?Sized>(
        &mut self,
        interval: Duration,
        steps: u32,
        marks: &FrameMarks,
        present: Duration,
        app: &A,
    ) {
        let Some(out) = self.out.as_mut() else {
            return;
        };
        self.stages.clear();
        app.frame_stages(&mut |name, ms| {
            if !OWN.contains(&name) {
                self.stages.push((name, ms));
            }
        });
        if self.frames == 0 {
            let _ = write!(
                out,
                "frame interval_ms steps update_ms frame_ms draw_ms present_ms"
            );
            for (name, _) in &self.stages {
                let unit = if name.ends_with(COUNT) { "" } else { "_ms" };
                let _ = write!(out, " {}{unit}", name.replace(' ', "_"));
            }
            let _ = writeln!(out);
        }
        self.frames = self.frames.saturating_add(1);
        let ms =
            |from: Instant, to: Instant| to.saturating_duration_since(from).as_secs_f64() * 1e3;
        let _ = write!(
            out,
            "{} {:.3} {steps} {:.3} {:.3} {:.3} {:.3}",
            self.frames,
            interval.as_secs_f64() * 1e3,
            ms(marks.start, marks.stepped),
            ms(marks.stepped, marks.framed),
            ms(marks.framed, marks.drawn),
            present.as_secs_f64() * 1e3,
        );
        for (name, value) in &self.stages {
            if name.ends_with(COUNT) {
                let _ = write!(out, " {value:.0}");
            } else {
                let _ = write!(out, " {value:.3}");
            }
        }
        let _ = writeln!(out);
        if self.frames % FLUSH_EVERY == 0 {
            let _ = out.flush();
        }
    }
}
