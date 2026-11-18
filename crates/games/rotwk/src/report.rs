use std::io::Write;

#[derive(Debug, Default)]
pub(crate) struct SecondReport {
    previous: Vec<String>,
    current: Vec<String>,
    out: String,
    printed: u64,
    skipped: u64,
}

impl SecondReport {
    pub(crate) fn line(&mut self, line: impl Into<String>) {
        self.current.push(line.into());
    }

    pub(crate) fn lines<I, S>(&mut self, lines: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for line in lines {
            self.line(line);
        }
    }

    pub(crate) fn flush(&mut self) -> bool {
        self.out.clear();
        for (index, line) in self.current.iter().enumerate() {
            if self.previous.get(index).is_some_and(|last| last == line) {
                self.skipped = self.skipped.saturating_add(1);
                continue;
            }
            self.printed = self.printed.saturating_add(1);
            self.out.push_str(line);
            self.out.push('\n');
        }
        let wrote = !self.out.is_empty();
        if wrote {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let _ = lock.write_all(self.out.as_bytes());
            let _ = lock.flush();
        }
        std::mem::swap(&mut self.previous, &mut self.current);
        self.current.clear();
        wrote
    }

    pub(crate) const fn census(&self) -> (u64, u64) {
        (self.printed, self.skipped)
    }
}

const FRAME_WINDOW: usize = 2_048;

#[derive(Debug)]
pub(crate) struct FrameTimes {
    samples: Vec<f32>,
    next: usize,
    filled: usize,
}

impl Default for FrameTimes {
    fn default() -> Self {
        Self {
            samples: vec![0.0; FRAME_WINDOW],
            next: 0,
            filled: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FrameSummary {
    pub(crate) count: usize,
    pub(crate) mean: f32,
    pub(crate) p50: f32,
    pub(crate) p99: f32,
    pub(crate) max: f32,
}

impl FrameTimes {
    pub(crate) fn record(&mut self, dt: f32) {
        if !dt.is_finite() || dt < 0.0 {
            return;
        }
        let Some(slot) = self.samples.get_mut(self.next) else {
            return;
        };
        *slot = dt * 1_000.0;
        self.next = self.next.saturating_add(1) % FRAME_WINDOW.max(1);
        self.filled = self.filled.saturating_add(1).min(FRAME_WINDOW);
    }

    pub(crate) fn reset(&mut self) {
        self.next = 0;
        self.filled = 0;
    }

    pub(crate) fn summary(&self) -> Option<FrameSummary> {
        let mut values: Vec<f32> = self.samples.get(..self.filled)?.to_vec();
        if values.is_empty() {
            return None;
        }
        values.sort_by(f32::total_cmp);
        let count = values.len();
        #[allow(clippy::cast_precision_loss)]
        let mean = values.iter().sum::<f32>() / count as f32;
        let at = |fraction: usize| -> f32 {
            let index = count
                .saturating_mul(fraction)
                .saturating_div(100)
                .min(count.saturating_sub(1));
            values.get(index).copied().unwrap_or(0.0)
        };
        Some(FrameSummary {
            count,
            mean,
            p50: at(50),
            p99: at(99),
            max: values.last().copied().unwrap_or(0.0),
        })
    }
}

pub(crate) fn frame_line(label: &str, summary: Option<FrameSummary>) -> String {
    match summary {
        Some(s) if s.mean > 0.0 => format!(
            "{label}: {} frame(s), mean {:.2} ms ({:.1} fps), p50 {:.2}, p99 {:.2}, max {:.2}",
            s.count,
            s.mean,
            1_000.0 / s.mean,
            s.p50,
            s.p99,
            s.max
        ),
        Some(s) => format!(
            "{label}: {} frame(s), mean {:.2} ms, p50 {:.2}, p99 {:.2}, max {:.2}",
            s.count, s.mean, s.p50, s.p99, s.max
        ),
        None => format!("{label}: no frames recorded"),
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn an_unchanged_line_is_not_written_a_second_time() {
        let mut report = SecondReport::default();
        report.line("a");
        report.line("b");
        assert!(report.flush());
        assert_eq!(report.census(), (2, 0));

        report.line("a");
        report.line("b");
        assert!(!report.flush());
        assert_eq!(report.census(), (2, 2));

        report.line("a");
        report.line("c");
        assert!(report.flush());
        assert_eq!(report.census(), (3, 3));
    }

    #[test]
    fn a_report_that_changes_shape_reprints_from_the_shift() {
        let mut report = SecondReport::default();
        report.lines(["a", "b", "c"]);
        assert!(report.flush());
        assert_eq!(report.census(), (3, 0));

        report.lines(["a", "new", "b", "c"]);
        assert!(report.flush());
        assert_eq!(report.census(), (6, 1));
    }

    #[test]
    fn an_empty_report_writes_nothing() {
        let mut report = SecondReport::default();
        assert!(!report.flush());
        assert_eq!(report.census(), (0, 0));
    }

    #[test]
    fn a_frame_window_summarises_what_it_was_given() {
        let mut times = FrameTimes::default();
        assert_eq!(times.summary(), None);
        for _ in 0..100 {
            times.record(0.001);
        }
        times.record(0.010);
        let summary = times.summary().unwrap();
        assert_eq!(summary.count, 101);
        assert!((summary.p50 - 1.0).abs() < 1e-3, "{summary:?}");
        assert!((summary.max - 10.0).abs() < 1e-3, "{summary:?}");
        assert!(summary.mean > 1.0 && summary.mean < 1.2, "{summary:?}");
        times.reset();
        assert_eq!(times.summary(), None);
    }

    #[test]
    fn a_hostile_frame_time_is_dropped_rather_than_recorded() {
        let mut times = FrameTimes::default();
        times.record(f32::NAN);
        times.record(-1.0);
        times.record(f32::INFINITY);
        assert_eq!(times.summary(), None);
        times.record(0.002);
        assert_eq!(times.summary().unwrap().count, 1);
    }

    #[test]
    fn the_window_is_bounded_by_its_own_ring() {
        let mut times = FrameTimes::default();
        for _ in 0..(FRAME_WINDOW * 3) {
            times.record(0.005);
        }
        assert_eq!(times.summary().unwrap().count, FRAME_WINDOW);
    }

    #[test]
    fn a_frame_line_survives_having_nothing_to_report() {
        assert!(frame_line("all frames", None).contains("no frames"));
    }
}
