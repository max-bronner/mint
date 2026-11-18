use crate::clock::{frame_at, ms_of_frame};
use crate::frame::{FramePlanes, FrameSource, SourceError, StreamInfo};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayFlags(pub u32);

impl PlayFlags {
    pub const NO_RENDER: u32 = 0x01;
    pub const LOOP: u32 = 0x04;
    pub const STRETCH: u32 = 0x40;

    fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tick {
    pub rendered: bool,
    pub decoded: u32,
    pub done: bool,
}

#[derive(Debug)]
pub struct MoviePlayer<S> {
    source: S,
    info: StreamInfo,
    flags: PlayFlags,
    skippable: bool,
    start_ms: i64,
    decoded: u32,
    shown: u32,
    dropped: u32,
    loops: u32,
    done: bool,
    failure: Option<SourceError>,
}

impl<S: FrameSource> MoviePlayer<S> {
    pub fn new(source: S, flags: PlayFlags, skippable: bool, now_ms: i64, latency_ms: u32) -> Self {
        let info = source.info();
        let mut player = Self {
            source,
            info,
            flags,
            skippable,
            start_ms: now_ms.saturating_add(i64::from(latency_ms)),
            decoded: 0,
            shown: 0,
            dropped: 0,
            loops: 0,
            done: info.frame_count == 0,
            failure: None,
        };
        player.predecode();
        player
    }

    fn predecode(&mut self) {
        if self.done {
            return;
        }
        match self.source.advance() {
            Ok(()) => self.decoded = 1,
            Err(error) => {
                self.failure = Some(error);
                self.done = true;
            }
        }
    }

    pub fn set_start(&mut self, now_ms: i64, latency_ms: u32) {
        self.start_ms = now_ms.saturating_add(i64::from(latency_ms));
    }

    pub fn update(&mut self, now_ms: i64) -> Tick {
        if self.done {
            return Tick {
                done: true,
                ..Tick::default()
            };
        }
        let elapsed = now_ms.saturating_sub(self.start_ms);
        let Some(due) = frame_at(
            elapsed,
            self.info.rate_numerator,
            self.info.rate_denominator,
        ) else {
            return Tick::default();
        };
        let count = u64::from(self.info.frame_count);
        let last_coded = count.saturating_sub(1);
        if due >= last_coded && u64::from(self.decoded) >= count {
            self.done = true;
            return Tick {
                done: true,
                ..Tick::default()
            };
        }
        let mut target = due.saturating_add(1).min(last_coded);
        let next = u64::from(self.decoded);
        if target > next && (self.flags.has(PlayFlags::STRETCH) || self.flags.has(PlayFlags::LOOP))
        {
            self.start_ms = now_ms.saturating_sub(ms_of_frame(
                next.saturating_sub(1),
                self.info.rate_numerator,
                self.info.rate_denominator,
            ));
            target = next;
        }
        let mut decoded = 0u32;
        while u64::from(self.decoded) <= target {
            if let Err(error) = self.source.advance() {
                self.failure = Some(error);
                self.done = true;
                break;
            }
            self.decoded = self.decoded.saturating_add(1);
            decoded = decoded.saturating_add(1);
        }
        let rendered = decoded > 0 && !self.flags.has(PlayFlags::NO_RENDER);
        if decoded > 0 {
            let unshown = if rendered {
                decoded.saturating_sub(1)
            } else {
                decoded
            };
            self.dropped = self.dropped.saturating_add(unshown);
            self.shown = self.shown.saturating_add(u32::from(rendered));
        }
        if self.decoded >= self.info.frame_count
            && self.flags.has(PlayFlags::LOOP)
            && self.failure.is_none()
        {
            self.source.rewind();
            self.decoded = 0;
            self.loops = self.loops.saturating_add(1);
            self.start_ms = now_ms;
            self.predecode();
        }
        Tick {
            rendered,
            decoded,
            done: self.done,
        }
    }

    pub fn skip(&mut self) -> bool {
        if self.skippable && !self.done {
            self.done = true;
            return true;
        }
        false
    }

    #[must_use]
    pub fn planes(&self) -> Option<FramePlanes<'_>> {
        self.source.planes()
    }

    #[must_use]
    pub fn info(&self) -> StreamInfo {
        self.info
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.done
    }

    #[must_use]
    pub fn frames_decoded(&self) -> u32 {
        self.decoded
    }

    #[must_use]
    pub fn frames_shown(&self) -> u32 {
        self.shown
    }

    #[must_use]
    pub fn frames_dropped(&self) -> u32 {
        self.dropped
    }

    #[must_use]
    pub fn loops(&self) -> u32 {
        self.loops
    }

    #[must_use]
    pub fn failure(&self) -> Option<&SourceError> {
        self.failure.as_ref()
    }

    #[must_use]
    pub fn source(&self) -> &S {
        &self.source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter {
        count: u32,
        at: u32,
    }

    impl FrameSource for Counter {
        fn info(&self) -> StreamInfo {
            StreamInfo {
                width: 16,
                height: 16,
                frame_count: self.count,
                rate_numerator: 983_010,
                rate_denominator: 32_767,
                has_alpha: false,
            }
        }
        fn advance(&mut self) -> Result<(), SourceError> {
            self.at += 1;
            Ok(())
        }
        fn planes(&self) -> Option<FramePlanes<'_>> {
            None
        }
        fn rewind(&mut self) {
            self.at = 0;
        }
    }

    fn player(flags: u32, skippable: bool) -> MoviePlayer<Counter> {
        let source = Counter { count: 90, at: 0 };
        MoviePlayer::new(source, PlayFlags(flags), skippable, 1_000, 700)
    }

    #[test]
    fn slot_zero_is_due_just_before_the_start_and_shows_the_second_coded_frame() {
        let mut p = player(0, false);
        assert_eq!(p.source().at, 1, "open decodes one frame");
        assert_eq!(p.update(1_660), Tick::default(), "40 ms early is too early");
        let tick = p.update(1_680);
        assert_eq!((tick.rendered, tick.decoded), (true, 1));
        assert_eq!(p.source().at, 2, "the frame shown is coded frame 1");
    }

    #[test]
    fn the_frame_is_the_elapsed_time_truncated_after_the_latency() {
        let mut p = player(0, false);
        let tick = p.update(1_799);
        assert_eq!((tick.rendered, tick.decoded), (true, 3));
        assert_eq!(p.source().at, 4);
        assert_eq!(p.frames_dropped(), 2);
    }

    #[test]
    fn a_frame_behind_the_clock_is_decoded_and_not_shown() {
        let mut p = player(0, false);
        let tick = p.update(2_700);
        assert_eq!(
            (tick.decoded, p.frames_shown(), p.frames_dropped()),
            (31, 1, 30)
        );
        let tick = p.update(2_700);
        assert!(!tick.rendered, "nothing new is due");
    }

    #[test]
    fn stretching_shows_every_frame_and_moves_the_clock() {
        let mut p = player(PlayFlags::STRETCH, false);
        p.update(1_700);
        let tick = p.update(3_000);
        assert_eq!((tick.decoded, p.frames_dropped()), (1, 0));
        assert_eq!(p.update(3_034).decoded, 1);
    }

    #[test]
    fn the_last_coded_frame_is_shown_once_and_the_next_update_ends() {
        let mut p = MoviePlayer::new(Counter { count: 4, at: 0 }, PlayFlags(0), false, 0, 0);
        let mut shown = 0;
        let mut now = 0;
        while !p.update(now).done {
            shown = p.frames_shown();
            now += 34;
            assert!(now < 1_000, "never ends");
        }
        assert_eq!((shown, p.source().at), (3, 4));
    }

    #[test]
    fn skip_ends_only_a_skippable_movie() {
        let mut logo = player(0, false);
        assert!(!logo.skip() && !logo.is_done());
        let mut intro = player(0, true);
        assert!(intro.skip() && intro.update(1_700).done);
    }

    #[test]
    fn a_loop_restarts_with_the_clock_reset_and_no_render_shows_nothing() {
        let mut p = player(PlayFlags::LOOP, false);
        for now in (1_700..40_000).step_by(33) {
            assert!(!p.update(now).done);
        }
        assert!(p.loops() >= 1);
        let mut silent = player(PlayFlags::NO_RENDER, false);
        let tick = silent.update(2_000);
        assert!(!tick.rendered && tick.decoded > 0);
    }
}
