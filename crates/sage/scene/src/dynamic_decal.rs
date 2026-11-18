use mint_render::{Mat4, Vec3};
use mint_sage_ini::{DecalOpacity, FxDynamicDecal};

const FRAMES_PER_MS: f32 = 0.03;

const MAX_SPAN_FRAMES: f32 = 2_592_000.0;

#[must_use]
pub fn frames(ms: f32) -> u32 {
    let frames = (ms * FRAMES_PER_MS).clamp(0.0, MAX_SPAN_FRAMES);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = frames as u32;
    whole
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecalTimeline {
    start: u32,
    fade_one_end: u32,
    peak_end: Option<u32>,
    fade_two_end: u32,
    remove_at: Option<u32>,
    opacity: [i64; 3],
}

impl DecalTimeline {
    #[must_use]
    pub fn fired(now: u32, decal: &FxDynamicDecal) -> Self {
        let start = now.saturating_add(frames(decal.starting_delay_ms));
        let [fade_one, peak_time, fade_two] = decal.fade_ms.map(frames);
        let fade_one_end = start.saturating_add(fade_one);
        let peak_end = fade_one_end.saturating_add(peak_time);
        Self {
            start,
            fade_one_end,
            peak_end: Some(peak_end),
            fade_two_end: peak_end.saturating_add(fade_two),
            remove_at: Some(now.saturating_add(frames(decal.lifetime_ms))),
            opacity: decal.opacity,
        }
    }

    #[must_use]
    pub fn shadow(now: u32, opacity: &DecalOpacity) -> Self {
        let fade_in = if opacity.dynamic {
            frames(opacity.fade_in_ms)
        } else {
            0
        };
        Self {
            start: now,
            fade_one_end: now.saturating_add(fade_in),
            peak_end: None,
            fade_two_end: frames(opacity.fade_out_ms),
            remove_at: None,
            opacity: [opacity.start, opacity.peak, opacity.end],
        }
    }

    #[must_use]
    pub fn removed(&self, now: u32, dynamic: bool) -> Option<Self> {
        if !dynamic {
            return None;
        }
        let fade = match self.peak_end {
            Some(peak_end) => self.fade_two_end.saturating_sub(peak_end),
            None => self.fade_two_end,
        };
        let end = now.saturating_add(fade);
        Some(Self {
            start: now,
            fade_one_end: now,
            peak_end: Some(now),
            fade_two_end: end,
            remove_at: Some(end),
            opacity: self.opacity,
        })
    }

    #[must_use]
    pub fn opacity_at(&self, now: u32) -> Option<i64> {
        if self.remove_at.is_some_and(|at| now >= at) {
            return None;
        }
        let [start, peak, end] = self.opacity;
        let ramp = |from: i64, to: i64, since: u32, until: u32| -> i64 {
            let span = until.saturating_sub(since).max(1);
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let value = (from as f32
                + (to - from) as f32 * (now.saturating_sub(since) as f32 / span as f32))
                .ceil() as i64;
            value
        };
        Some(if now < self.start {
            0
        } else if now < self.fade_one_end {
            ramp(start, peak, self.start, self.fade_one_end)
        } else if self.peak_end.is_none_or(|peak_end| now < peak_end) {
            peak
        } else if now < self.fade_two_end {
            ramp(
                peak,
                end,
                self.peak_end.unwrap_or(self.fade_two_end),
                self.fade_two_end,
            )
        } else {
            end
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FiredDecal {
    pub decal: FxDynamicDecal,
    pub centre: Vec3,
    pub facing: f32,
}

impl FiredDecal {
    #[must_use]
    pub fn place(decal: &FxDynamicDecal, at: Vec3, matrix: Option<Mat4>) -> Self {
        let facing = match matrix {
            Some(matrix) if decal.orient_to_object => {
                let forward = matrix.transform_vector(Vec3::new(1.0, 0.0, 0.0));
                mint_math::f32::atan2(forward.y, forward.x)
            }
            _ => 0.0,
        };
        let [x, y] = decal.offset;
        let (sin, cos) = (mint_math::f32::sin(facing), mint_math::f32::cos(facing));
        Self {
            decal: decal.clone(),
            centre: Vec3::new(at.x + x * cos - y * sin, at.y + x * sin + y * cos, at.z),
            facing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quake() -> FxDynamicDecal {
        FxDynamicDecal {
            texture: "EXQuake01".to_owned(),
            shader: mint_sage_ini::DecalShader::Alpha,
            size: 400.0,
            color: [225.0, 205.0, 185.0],
            offset: [0.0, 0.0],
            orient_to_object: true,
            opacity: [0, 130, 0],
            fade_ms: [150.0, 7000.0, 9000.0],
            starting_delay_ms: 1500.0,
            lifetime_ms: 25000.0,
        }
    }

    #[test]
    fn a_fired_decal_rises_holds_falls_and_goes_on_its_own_frames() {
        let line = DecalTimeline::fired(100, &quake());
        assert_eq!(line.opacity_at(144), Some(0));
        assert_eq!(line.opacity_at(147), Some(65));
        assert_eq!(line.opacity_at(149), Some(130));
        assert_eq!(line.opacity_at(358), Some(130));
        assert_eq!(line.opacity_at(359 + 135), Some(65));
        assert_eq!(line.opacity_at(849), Some(0));
        assert_eq!(line.opacity_at(850), None);
    }

    #[test]
    fn a_dynamic_shadow_fades_in_holds_and_fades_out_after_removal() {
        let eye = DecalOpacity {
            dynamic: true,
            start: 0,
            fade_in_ms: 1000.0,
            peak: 255,
            fade_out_ms: 1000.0,
            end: 0,
        };
        let shown = DecalTimeline::shadow(10, &eye);
        assert_eq!(shown.opacity_at(25), Some(128));
        assert_eq!(shown.opacity_at(10_000), Some(255));
        let gone = shown.removed(20, true).unwrap();
        assert_eq!(gone.opacity_at(35), Some(128));
        assert_eq!(gone.opacity_at(50), None);
        assert_eq!(shown.removed(20, false), None);
    }
}
