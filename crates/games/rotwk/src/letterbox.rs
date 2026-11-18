use mint_app::RtsCamera;

const FADE_SECONDS: f32 = 1.0;

pub(crate) type Rect = [f32; 4];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bars {
    pub(crate) top: Rect,
    pub(crate) bottom: Option<Rect>,
    pub(crate) alpha: f32,
}

pub(crate) fn bar_height(width: f32, height: f32) -> f32 {
    (height - 0.5625 * width) * 0.5
}

#[derive(Debug, Default)]
pub(crate) struct Letterbox {
    on: bool,
    level: f32,
    saved_range: Option<(f32, f32)>,
}

impl Letterbox {
    pub(crate) fn update(&mut self, on: bool, dt: f32, camera: &mut RtsCamera) {
        if on != self.on {
            self.on = on;
            if on {
                let old = camera.distance_range();
                self.saved_range = Some(old);
                let _ = camera
                    .set_distance_range(RtsCamera::DEFAULT_DISTANCE_RANGE.0.min(old.0), 1.0e5);
            } else if let Some((min, max)) = self.saved_range.take() {
                let _ = camera.set_distance_range(min, max);
            }
        }
        let step = dt.max(0.0) / FADE_SECONDS;
        self.level = if on {
            (self.level + step).min(1.0)
        } else {
            (self.level - step).max(0.0)
        };
    }

    pub(crate) fn bars(&self, width: f32, height: f32) -> Option<Bars> {
        let bar = bar_height(width, height);
        if self.level <= 0.0 || bar <= 0.0 {
            return None;
        }
        Some(Bars {
            top: [0.0, 0.0, width, bar],
            bottom: self.on.then_some([0.0, height - bar, width, height]),
            alpha: self.level,
        })
    }
}

#[cfg(test)]
mod tests {
    use mint_render::Vec3;

    use super::*;

    #[test]
    fn a_4_3_display_has_a_bar_of_an_eighth_of_its_height_and_a_wide_one_has_none() {
        assert!((bar_height(1024.0, 768.0) - 96.0).abs() < 1e-3);
        assert!(bar_height(1920.0, 1080.0) <= 0.0);
        assert!(bar_height(2560.0, 1080.0) < 0.0);
    }

    #[test]
    fn the_bars_fade_in_over_a_second_and_only_the_top_one_fades_out() {
        let mut camera = RtsCamera::new(Vec3::Z);
        let mut letterbox = Letterbox::default();
        letterbox.update(true, 0.5, &mut camera);
        let half = letterbox.bars(1024.0, 768.0).expect("bars");
        assert!((half.alpha - 0.5).abs() < 1e-5 && half.bottom.is_some());
        letterbox.update(true, 0.75, &mut camera);
        assert_eq!(letterbox.bars(1024.0, 768.0).map(|b| b.alpha), Some(1.0));
        letterbox.update(false, 0.25, &mut camera);
        let fading = letterbox.bars(1024.0, 768.0).expect("bars");
        assert!(fading.bottom.is_none() && (fading.alpha - 0.75).abs() < 1e-5);
        letterbox.update(false, 1.0, &mut camera);
        assert!(letterbox.bars(1024.0, 768.0).is_none());
    }

    #[test]
    fn the_zoom_range_is_lifted_while_on_and_restored_after() {
        let mut camera = RtsCamera::new(Vec3::Z);
        assert!(camera.set_distance_range(50.0, 400.0));
        let mut letterbox = Letterbox::default();
        letterbox.update(true, 0.1, &mut camera);
        assert!(camera.distance_range().1 > 400.0);
        letterbox.update(false, 0.1, &mut camera);
        assert_eq!(camera.distance_range(), (50.0, 400.0));
    }
}
