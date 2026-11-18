pub(crate) const CLIENT_FRAME_SECONDS: f32 = 1.0 / 30.0;

const MAX_CLIENT_FRAME_CATCHUP: u16 = 900;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn client_frames_elapsed(carry: &mut f32, dt: f32) -> u16 {
    *carry += dt.max(0.0);
    let uncapped = (*carry / CLIENT_FRAME_SECONDS + 1.0e-4).floor().max(0.0);
    if uncapped > f32::from(MAX_CLIENT_FRAME_CATCHUP) {
        *carry = 0.0;
        return MAX_CLIENT_FRAME_CATCHUP;
    }
    let steps = uncapped as u16;
    *carry -= f32::from(steps) * CLIENT_FRAME_SECONDS;
    if !carry.is_finite() || *carry < 0.0 {
        *carry = 0.0;
    }
    steps
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn exact_multiples_of_the_frame_time_step_cleanly() {
        let mut carry = 0.0;
        assert_eq!(client_frames_elapsed(&mut carry, 1.0), 30);
        assert_eq!(carry, 0.0);
    }

    #[test]
    fn leftover_time_carries_to_the_next_call() {
        let mut carry = 0.0;
        assert_eq!(
            client_frames_elapsed(&mut carry, CLIENT_FRAME_SECONDS * 1.5),
            1
        );
        assert!(carry > 0.0 && carry < CLIENT_FRAME_SECONDS);
        assert_eq!(
            client_frames_elapsed(&mut carry, CLIENT_FRAME_SECONDS * 0.5),
            1
        );
    }

    #[test]
    fn a_long_stall_is_capped_rather_than_spun_through() {
        let mut carry = 0.0;
        assert_eq!(client_frames_elapsed(&mut carry, 1000.0), 900);
        assert_eq!(carry, 0.0, "the backlog past the cap is dropped, not kept");
    }
}
