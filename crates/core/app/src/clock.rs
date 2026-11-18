use std::time::Duration;

pub use mint_sim::SIMULATION_HZ;

const MAX_STEPS_PER_ADVANCE: u32 = 8;

#[derive(Debug, Clone)]
pub struct FixedTimestep {
    step: Duration,
    accumulated: Duration,
    steps_taken: u64,
}

impl Default for FixedTimestep {
    fn default() -> Self {
        Self::new(SIMULATION_HZ)
    }
}

impl FixedTimestep {
    #[must_use]
    pub fn new(hz: u32) -> Self {
        let hz = if hz == 0 { SIMULATION_HZ } else { hz };
        Self {
            step: Duration::from_nanos(1_000_000_000u64.checked_div(u64::from(hz)).unwrap_or(0)),
            accumulated: Duration::ZERO,
            steps_taken: 0,
        }
    }

    #[must_use]
    pub fn step(&self) -> Duration {
        self.step
    }

    #[must_use]
    pub fn steps_taken(&self) -> u64 {
        self.steps_taken
    }

    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        self.advance_scaled(elapsed, 1.0)
    }

    pub fn advance_scaled(&mut self, elapsed: Duration, speed: f64) -> u32 {
        let speed = if speed.is_finite() && speed > 0.0 {
            speed
        } else {
            1.0
        };
        let scaled = Duration::try_from_secs_f64(elapsed.as_secs_f64() * speed).unwrap_or(elapsed);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cap = ((f64::from(MAX_STEPS_PER_ADVANCE) * speed).ceil() as u32).max(1);
        self.accumulated = self.accumulated.saturating_add(scaled);
        if self.step.is_zero() {
            return 0;
        }

        let mut steps = 0u32;
        while self.accumulated >= self.step && steps < cap {
            self.accumulated = self.accumulated.saturating_sub(self.step);
            steps = steps.saturating_add(1);
        }
        if self.accumulated >= self.step {
            self.accumulated = Duration::ZERO;
        }
        self.steps_taken = self.steps_taken.saturating_add(u64::from(steps));
        steps
    }

    pub fn advance_unless_paused(&mut self, elapsed: Duration, speed: f64, paused: bool) -> u32 {
        if paused {
            0
        } else {
            self.advance_scaled(elapsed, speed)
        }
    }

    #[must_use]
    pub fn blend(&self) -> f32 {
        if self.step.is_zero() {
            return 0.0;
        }
        let ratio = self.accumulated.as_secs_f64() / self.step.as_secs_f64();
        #[allow(clippy::cast_possible_truncation)]
        let narrowed = ratio.clamp(0.0, 1.0) as f32;
        narrowed
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    #[test]
    fn the_default_rate_is_the_one_the_shipped_data_assumes() {
        assert_eq!(SIMULATION_HZ, 30);
        assert_eq!(
            FixedTimestep::default().step(),
            Duration::from_nanos(33_333_333)
        );
    }

    #[test]
    fn a_frame_shorter_than_a_step_runs_no_steps() {
        let mut clock = FixedTimestep::new(30);
        assert_eq!(clock.advance(Duration::from_micros(6944)), 0);
        assert_eq!(clock.steps_taken(), 0);
    }

    #[test]
    fn several_short_frames_eventually_add_up_to_one_step() {
        let mut clock = FixedTimestep::new(30);
        let frame = Duration::from_micros(6944);
        let mut total = 0u32;
        for _ in 0..5 {
            total += clock.advance(frame);
        }
        assert_eq!(total, 1, "five 144 Hz frames exceed one 30 Hz tick");
    }

    #[test]
    fn a_long_frame_runs_several_steps_at_once() {
        let mut clock = FixedTimestep::new(30);
        assert_eq!(clock.advance(Duration::from_millis(100)), 3);
        assert_eq!(clock.steps_taken(), 3);
    }

    #[test]
    fn a_stall_is_capped_rather_than_spiralling() {
        let mut clock = FixedTimestep::new(30);
        assert_eq!(clock.advance(Duration::from_secs(10)), 8);
        assert_eq!(clock.advance(Duration::ZERO), 0);
        assert!(clock.blend() < 1.0);
    }

    #[test]
    fn a_speed_multiplies_the_steps_a_frame_owes() {
        let mut clock = FixedTimestep::new(30);
        assert_eq!(clock.advance_scaled(Duration::from_millis(100), 5.0), 15);
        assert_eq!(clock.advance_scaled(Duration::from_millis(100), 0.25), 0);
    }

    #[test]
    fn the_step_cap_scales_with_the_speed_and_stays_at_eight_at_one() {
        let mut clock = FixedTimestep::new(30);
        assert_eq!(clock.advance_scaled(Duration::from_secs(10), 60.0), 480);
        assert_eq!(clock.advance_scaled(Duration::from_secs(10), 1.0), 8);
        assert_eq!(clock.advance_scaled(Duration::from_secs(10), 0.1), 1);
    }

    #[test]
    fn blend_reports_progress_towards_the_next_step() {
        let mut clock = FixedTimestep::new(50);
        assert_eq!(clock.blend(), 0.0);
        clock.advance(clock.step() / 2);
        assert!(
            (clock.blend() - 0.5).abs() < 1e-3,
            "blend {}",
            clock.blend()
        );
        clock.advance(clock.step() / 2);
        assert!(
            clock.blend() < 1e-3,
            "a whole step should have been consumed"
        );
        assert_eq!(clock.steps_taken(), 1);
    }

    #[test]
    fn blend_never_leaves_the_zero_to_one_range() {
        let mut clock = FixedTimestep::new(30);
        clock.advance(Duration::from_secs(60));
        let blend = clock.blend();
        assert!((0.0..=1.0).contains(&blend), "blend {blend}");
    }

    #[test]
    fn a_zero_rate_falls_back_rather_than_dividing_by_zero() {
        let clock = FixedTimestep::new(0);
        assert_eq!(clock.step(), FixedTimestep::default().step());
    }

    #[test]
    fn paused_holds_blend_still_instead_of_wobbling_on_discarded_steps() {
        let mut clock = FixedTimestep::new(30);
        clock.advance(clock.step() / 2);
        let held = clock.blend();
        for _ in 0..5 {
            assert_eq!(clock.advance_unless_paused(clock.step() / 4, 1.0, true), 0);
            assert_eq!(clock.blend(), held, "blend must not move while paused");
        }
        assert_eq!(clock.steps_taken(), 0);
        assert_eq!(clock.advance_unless_paused(clock.step(), 1.0, false), 1);
    }

    #[test]
    fn the_rate_is_configurable_because_it_is_not_a_law_of_nature() {
        let mut clock = FixedTimestep::new(60);
        assert_eq!(clock.step(), Duration::from_nanos(16_666_666));
        assert_eq!(clock.advance(Duration::from_millis(50)), 3);
    }
}
