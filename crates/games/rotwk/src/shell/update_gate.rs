const MIN_GAP_MS: f32 = 32.333;

#[derive(Debug, Default)]
pub(crate) struct UpdateGate {
    last_ms: u32,
}

impl UpdateGate {
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn due(&mut self, now_ms: u32) -> bool {
        let gap = now_ms.wrapping_sub(self.last_ms);
        if gap as f32 >= MIN_GAP_MS {
            self.last_ms = now_ms;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UpdateGate;

    fn runs(fps: f32, frames: u32) -> Vec<u32> {
        let mut gate = UpdateGate::default();
        (1..=frames)
            .filter(|i| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let now = (*i as f32 * 1000.0 / fps).round() as u32;
                gate.due(now)
            })
            .collect()
    }

    #[test]
    fn sixty_and_forty_fps_update_every_second_frame() {
        assert_eq!(runs(60.0, 8), vec![2, 4, 6, 8]);
        assert_eq!(runs(40.0, 8), vec![2, 4, 6, 8]);
    }

    #[test]
    fn the_reference_is_the_run_time_not_a_fixed_step() {
        let mut gate = UpdateGate::default();
        assert!(gate.due(40));
        assert!(!gate.due(65));
        assert!(gate.due(73));
    }
}
