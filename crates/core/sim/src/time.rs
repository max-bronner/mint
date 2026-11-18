use core::num::NonZeroU32;

use mint_ecs::StateHasher;

pub const SIMULATION_HZ: u32 = 30;

#[allow(clippy::cast_precision_loss)]
const SECONDS_PER_FRAME: f32 = 1.0 / SIMULATION_HZ as f32;

const MSEC_PER_SECOND: u64 = 1_000;

const MAX_AUTHORED_MILLISECONDS: f32 = 86_400_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct LogicFrames(u32);

impl LogicFrames {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);
    pub const SECOND: Self = Self(SIMULATION_HZ);
    pub const MAX: Self = Self(u32::MAX);

    #[must_use]
    pub const fn from_frames(frames: u32) -> Self {
        Self(frames)
    }

    #[must_use]
    pub const fn from_engine_milliseconds(milliseconds: u32) -> Self {
        let scaled = (milliseconds as u64).saturating_mul(SIMULATION_HZ as u64);
        let rounded = match scaled
            .saturating_add(MSEC_PER_SECOND / 2)
            .checked_div(MSEC_PER_SECOND)
        {
            Some(frames) => frames,
            None => 0,
        };
        Self::saturating_from_u64(rounded)
    }

    #[must_use]
    pub const fn from_authored_milliseconds(milliseconds: u32, authored_hz: NonZeroU32) -> Self {
        let hz = authored_hz.get() as u64;
        let authored_frames = (milliseconds as u64)
            .saturating_mul(hz)
            .div_ceil(MSEC_PER_SECOND);
        let ticks = authored_frames
            .saturating_mul(SIMULATION_HZ as u64)
            .div_ceil(hz);
        Self::saturating_from_u64(ticks)
    }

    #[must_use]
    pub fn from_authored_milliseconds_f32(milliseconds: f32, authored_hz: NonZeroU32) -> Self {
        if !milliseconds.is_finite() || milliseconds <= 0.0 {
            return Self::ZERO;
        }
        let bounded = milliseconds.min(MAX_AUTHORED_MILLISECONDS);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = bounded.ceil() as u32;
        Self::from_authored_milliseconds(whole, authored_hz)
    }

    #[must_use]
    pub const fn from_authored_frames(frames: u32, authored_hz: NonZeroU32) -> Self {
        let ticks = (frames as u64)
            .saturating_mul(SIMULATION_HZ as u64)
            .div_ceil(authored_hz.get() as u64);
        Self::saturating_from_u64(ticks)
    }

    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn authored_frames_truncated(seconds: f32, authored_hz: NonZeroU32) -> u32 {
        if !seconds.is_finite() || seconds <= 0.0 {
            return 0;
        }
        let bounded = seconds.min(MAX_AUTHORED_MILLISECONDS / 1000.0);
        (bounded * authored_hz.get() as f32) as u32
    }

    #[must_use]
    pub fn from_authored_seconds_truncated(seconds: f32, authored_hz: NonZeroU32) -> Self {
        Self::from_authored_frames(
            Self::authored_frames_truncated(seconds, authored_hz),
            authored_hz,
        )
    }

    const fn saturating_from_u64(ticks: u64) -> Self {
        if ticks > u32::MAX as u64 {
            return Self::MAX;
        }
        #[allow(clippy::cast_possible_truncation)]
        Self(ticks as u32)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(sum) => Some(Self(sum)),
            None => None,
        }
    }

    #[must_use]
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(difference) => Some(Self(difference)),
            None => None,
        }
    }

    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_seconds(self) -> f32 {
        self.0 as f32 * SECONDS_PER_FRAME
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn from_seconds(seconds: f32) -> Self {
        if seconds.is_nan() || seconds <= 0.0 {
            return Self::ZERO;
        }
        let frames = (seconds * SIMULATION_HZ as f32).round();
        if frames >= u32::MAX as f32 {
            return Self::MAX;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Self(frames as u32)
    }

    #[must_use]
    pub const fn as_milliseconds(self) -> u64 {
        let scaled = (self.0 as u64).saturating_mul(MSEC_PER_SECOND);
        match scaled.checked_div(SIMULATION_HZ as u64) {
            Some(milliseconds) => milliseconds,
            None => 0,
        }
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0);
    }
}

impl core::fmt::Display for LogicFrames {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}f", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Frame(u64);

impl Frame {
    pub const ZERO: Self = Self(0);

    pub const FOREVER: Self = Self(u64::MAX);

    #[must_use]
    pub const fn new(index: u64) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(index) => Some(Self(index)),
            None => None,
        }
    }

    #[must_use]
    pub const fn checked_add(self, duration: LogicFrames) -> Option<Self> {
        match self.0.checked_add(duration.get() as u64) {
            Some(index) => Some(Self(index)),
            None => None,
        }
    }

    #[must_use]
    pub fn since(self, earlier: Self) -> Option<LogicFrames> {
        let gap = self.0.checked_sub(earlier.0)?;
        u32::try_from(gap).ok().map(LogicFrames::from_frames)
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0);
    }
}

impl core::fmt::Display for Frame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "frame {}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tick_rate_is_the_one_the_shipped_data_assumes() {
        assert_eq!(SIMULATION_HZ, 30);
        assert_eq!(LogicFrames::SECOND.get(), 30);
    }

    #[test]
    fn a_shipped_duration_converts_to_the_seconds_its_comment_claims() {
        let reaction = LogicFrames::from_frames(90);
        assert!((reaction.as_seconds() - 3.0).abs() < 1e-6);
        assert_eq!(LogicFrames::from_seconds(3.0), reaction);
    }

    #[test]
    fn milliseconds_round_halves_up_on_an_exact_half_frame() {
        assert_eq!(
            LogicFrames::from_engine_milliseconds(50),
            LogicFrames::from_frames(2)
        );
        assert_eq!(
            LogicFrames::from_engine_milliseconds(150),
            LogicFrames::from_frames(5)
        );
        assert_eq!(
            LogicFrames::from_engine_milliseconds(250),
            LogicFrames::from_frames(8)
        );
        assert_eq!(
            LogicFrames::from_engine_milliseconds(49),
            LogicFrames::from_frames(1)
        );
        assert_eq!(
            LogicFrames::from_engine_milliseconds(51),
            LogicFrames::from_frames(2)
        );
    }

    #[test]
    fn milliseconds_convert_at_the_rate_the_original_engine_declares() {
        assert_eq!(
            LogicFrames::from_engine_milliseconds(1_000),
            LogicFrames::SECOND,
            "one second of milliseconds is one second of frames"
        );
        assert_eq!(LogicFrames::from_engine_milliseconds(0), LogicFrames::ZERO);
        assert_eq!(
            LogicFrames::from_engine_milliseconds(3_000),
            LogicFrames::from_frames(90),
            "the 90-frame reaction time, expressed the other way"
        );
        assert_eq!(
            LogicFrames::from_engine_milliseconds(u32::MAX),
            LogicFrames::from_frames(128_849_019)
        );
    }

    #[test]
    fn authored_milliseconds_ceil_to_the_authored_frame_then_scale() {
        let hz = NonZeroU32::new(5).unwrap();
        let ticks = |ms| LogicFrames::from_authored_milliseconds(ms, hz).get();
        assert_eq!(ticks(0), 0);
        assert_eq!(ticks(5), 6, "one 200 ms frame, not the 1 tick of 30 Hz");
        assert_eq!(ticks(40), 6, "round-to-nearest would give 0");
        assert_eq!(ticks(200), 6);
        assert_eq!(ticks(210), 12, "round-to-nearest would give 6");
        assert_eq!(ticks(u32::MAX), u32::MAX / 200 * 6 + 6);
        let float = |ms| LogicFrames::from_authored_milliseconds_f32(ms, hz).get();
        assert_eq!((float(40.0), float(200.5), float(-1.0)), (6, 12, 0));
        let own = NonZeroU32::new(SIMULATION_HZ).unwrap();
        assert_eq!(LogicFrames::from_authored_milliseconds(50, own).get(), 2);
    }

    #[test]
    fn authored_seconds_truncate_to_the_authored_frame_then_scale() {
        let hz = NonZeroU32::new(5).unwrap();
        let ticks = |s| LogicFrames::from_authored_seconds_truncated(s, hz).get();
        assert_eq!((ticks(0.3), ticks(2.5), ticks(0.19)), (6, 72, 0));
        assert_eq!(
            (ticks(-1.0), ticks(f32::NAN), ticks(f32::INFINITY)),
            (0, 0, 0)
        );
        assert_eq!(LogicFrames::from_authored_frames(7, hz).get(), 42);
    }

    #[test]
    fn milliseconds_out_truncate_and_say_so() {
        assert_eq!(LogicFrames::ONE.as_milliseconds(), 33, "100/3 truncated");
        assert_eq!(
            LogicFrames::from_frames(2).as_milliseconds(),
            66,
            "200/3 truncated, not rounded to 67"
        );
        assert_eq!(LogicFrames::SECOND.as_milliseconds(), 1_000);
        assert_eq!(LogicFrames::ZERO.as_milliseconds(), 0);
        assert_eq!(LogicFrames::MAX.as_milliseconds(), 143_165_576_500);
    }

    #[test]
    fn the_three_constructors_agree_where_the_units_do() {
        assert_eq!(
            LogicFrames::from_seconds(3.0),
            LogicFrames::from_engine_milliseconds(3_000)
        );
        assert_eq!(LogicFrames::from_seconds(3.0), LogicFrames::from_frames(90));
        assert_eq!(
            LogicFrames::from_seconds(0.05),
            LogicFrames::from_engine_milliseconds(50)
        );
    }

    #[test]
    fn seconds_conversion_refuses_nonsense_rather_than_wrapping() {
        assert_eq!(LogicFrames::from_seconds(f32::NAN), LogicFrames::ZERO);
        assert_eq!(LogicFrames::from_seconds(-5.0), LogicFrames::ZERO);
        assert_eq!(LogicFrames::from_seconds(f32::INFINITY), LogicFrames::MAX);
        assert_eq!(LogicFrames::from_seconds(1e12), LogicFrames::MAX);
    }

    #[test]
    fn duration_arithmetic_states_its_overflow_behaviour() {
        assert_eq!(
            LogicFrames::MAX.checked_add(LogicFrames::ONE),
            None,
            "checked_add must report overflow rather than wrap"
        );
        assert_eq!(
            LogicFrames::MAX.saturating_add(LogicFrames::ONE),
            LogicFrames::MAX
        );
        assert_eq!(LogicFrames::ZERO.checked_sub(LogicFrames::ONE), None);
        assert_eq!(
            LogicFrames::ZERO.saturating_sub(LogicFrames::ONE),
            LogicFrames::ZERO
        );
        assert_eq!(
            LogicFrames::from_frames(7).checked_add(LogicFrames::from_frames(3)),
            Some(LogicFrames::from_frames(10))
        );
    }

    #[test]
    fn a_timer_counts_down_to_zero_and_stops() {
        let mut remaining = LogicFrames::from_frames(3);
        let mut steps = 0u32;
        while !remaining.is_zero() {
            remaining = remaining.saturating_sub(LogicFrames::ONE);
            steps = steps.saturating_add(1);
        }
        assert_eq!(steps, 3);
        assert_eq!(remaining, LogicFrames::ZERO);
    }

    #[test]
    fn frames_and_durations_are_different_types_that_compose() {
        let start = Frame::new(100);
        let later = start.checked_add(LogicFrames::from_frames(90));
        assert_eq!(later, Some(Frame::new(190)));
        assert_eq!(
            Frame::new(190).since(start),
            Some(LogicFrames::from_frames(90)),
            "the difference of two points is a duration"
        );
        assert_eq!(start.since(Frame::new(190)), None, "no negative durations");
    }

    #[test]
    fn the_end_of_the_timeline_is_reported_rather_than_wrapped() {
        assert_eq!(Frame::new(u64::MAX).next(), None);
        assert_eq!(Frame::new(u64::MAX).checked_add(LogicFrames::ONE), None);
        assert_eq!(Frame::new(5).next(), Some(Frame::new(6)));
    }

    #[test]
    fn a_gap_too_wide_for_a_duration_is_none_rather_than_truncated() {
        let far = Frame::new(u64::from(u32::MAX).saturating_add(1));
        assert_eq!(far.since(Frame::ZERO), None);
        assert_eq!(
            Frame::new(u64::from(u32::MAX)).since(Frame::ZERO),
            Some(LogicFrames::MAX)
        );
    }

    #[test]
    fn durations_hash_by_value() {
        let mut a = StateHasher::new();
        LogicFrames::from_frames(5).hash_into(&mut a);
        let mut b = StateHasher::new();
        LogicFrames::from_frames(6).hash_into(&mut b);
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn display_is_terse_enough_to_paste_into_a_report() {
        assert_eq!(LogicFrames::from_frames(90).to_string(), "90f");
        assert_eq!(Frame::new(12).to_string(), "frame 12");
    }
}
