use mint_sage_model::animation::frame_at;
use mint_sage_model::{Completion, ModelAnimation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blending {
    #[default]
    Retail,
    Off,
}

impl Blending {
    pub const ALL: [Self; 2] = [Self::Retail, Self::Off];

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Retail => Self::Off,
            Self::Off => Self::Retail,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Retail => "retail (AnimationBlendTime frames of the incoming clip)",
            Self::Off => "off (a cut on every clip change)",
        }
    }
}

pub const DEFAULT_BLEND_FRAMES: f32 = 5.0;

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn blend_seconds(declared: Option<f32>, frames: u32, rate: u32, speed: f32) -> f32 {
    let value = declared.unwrap_or(DEFAULT_BLEND_FRAMES);
    if value == 0.0 {
        return 0.0;
    }
    let value = if value > 0.0 {
        value
    } else {
        DEFAULT_BLEND_FRAMES
    };
    let blended = value.min(frames.saturating_sub(1) as f32).max(1.0);
    let per_second = rate as f32 * speed;
    if per_second.is_finite() && per_second > 0.0 {
        blended / per_second
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Slot {
    pub(super) state: usize,
    pub(super) clip: usize,
    frame: f32,
    seconds: f32,
}

impl Slot {
    pub(super) fn at((state, clip, frame): (usize, usize, f32)) -> Self {
        Self {
            state,
            clip,
            frame,
            seconds: 0.0,
        }
    }

    pub(super) fn advance(&mut self, seconds: f32) {
        self.seconds += seconds;
    }

    pub(super) fn frame(&self, animation: &ModelAnimation) -> f32 {
        frame_at(
            animation.mode,
            Completion::Hold,
            animation.frames,
            animation.rate,
            self.frame,
            self.seconds,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Blend {
    pub(super) from: Slot,
    pub(super) into: Option<Slot>,
    pub(super) started: f32,
    pub(super) duration: f32,
}

impl Blend {
    pub(super) fn weight(&self, now: f32) -> f32 {
        let weight = (now - self.started) / self.duration;
        if weight.is_finite() {
            weight.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}
