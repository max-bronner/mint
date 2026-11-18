use core::f32::consts::TAU;

#[must_use]
#[inline]
pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}

#[must_use]
#[inline]
pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}

#[must_use]
#[inline]
pub fn tan(x: f32) -> f32 {
    libm::tanf(x)
}

#[must_use]
#[inline]
pub fn sin_cos(x: f32) -> (f32, f32) {
    libm::sincosf(x)
}

#[must_use]
#[inline]
pub fn asin(x: f32) -> f32 {
    libm::asinf(x)
}

#[must_use]
#[inline]
pub fn acos(x: f32) -> f32 {
    libm::acosf(x)
}

#[must_use]
#[inline]
pub fn atan(x: f32) -> f32 {
    libm::atanf(x)
}

#[must_use]
#[inline]
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

#[must_use]
#[inline]
pub fn exp(x: f32) -> f32 {
    libm::expf(x)
}

#[must_use]
#[inline]
pub fn exp2(x: f32) -> f32 {
    libm::exp2f(x)
}

#[must_use]
#[inline]
pub fn exp_m1(x: f32) -> f32 {
    libm::expm1f(x)
}

#[must_use]
#[inline]
pub fn ln(x: f32) -> f32 {
    libm::logf(x)
}

#[must_use]
#[inline]
pub fn ln_1p(x: f32) -> f32 {
    libm::log1pf(x)
}

#[must_use]
#[inline]
pub fn log2(x: f32) -> f32 {
    libm::log2f(x)
}

#[must_use]
#[inline]
pub fn log10(x: f32) -> f32 {
    libm::log10f(x)
}

#[must_use]
#[inline]
pub fn log(x: f32, base: f32) -> f32 {
    libm::logf(x) / libm::logf(base)
}

#[must_use]
#[inline]
pub fn powf(x: f32, y: f32) -> f32 {
    libm::powf(x, y)
}

#[must_use]
pub fn powi(x: f32, n: i32) -> f32 {
    let mut acc: f32 = 1.0;
    let mut base = x;
    let mut exponent = n.unsigned_abs();

    while exponent > 0 {
        if exponent & 1 == 1 {
            acc *= base;
        }
        exponent = exponent.wrapping_shr(1);
        if exponent > 0 {
            base *= base;
        }
    }

    if n < 0 {
        1.0 / acc
    } else {
        acc
    }
}

#[must_use]
#[inline]
pub fn cbrt(x: f32) -> f32 {
    libm::cbrtf(x)
}

#[must_use]
#[inline]
pub fn hypot(x: f32, y: f32) -> f32 {
    libm::hypotf(x, y)
}

#[must_use]
#[inline]
pub fn sinh(x: f32) -> f32 {
    libm::sinhf(x)
}

#[must_use]
#[inline]
pub fn cosh(x: f32) -> f32 {
    libm::coshf(x)
}

#[must_use]
#[inline]
pub fn tanh(x: f32) -> f32 {
    libm::tanhf(x)
}

#[must_use]
#[inline]
pub fn asinh(x: f32) -> f32 {
    libm::asinhf(x)
}

#[must_use]
#[inline]
pub fn acosh(x: f32) -> f32 {
    libm::acoshf(x)
}

#[must_use]
#[inline]
pub fn atanh(x: f32) -> f32 {
    libm::atanhf(x)
}

#[must_use]
#[inline]
pub fn wrap_pi(radians: f32) -> f32 {
    libm::remainderf(radians, TAU)
}

#[must_use]
pub fn wrap_tau(radians: f32) -> f32 {
    let r = libm::fmodf(radians, TAU);
    if r < 0.0 {
        let shifted = r + TAU;
        if shifted >= TAU {
            0.0
        } else {
            shifted
        }
    } else {
        r
    }
}

#[must_use]
#[inline]
pub fn angle_delta(from: f32, to: f32) -> f32 {
    wrap_pi(to - from)
}

#[must_use]
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (1.0 - t) * a + t * b
}

#[must_use]
#[inline]
pub fn clamp(x: f32, min: f32, max: f32) -> f32 {
    if x < min {
        min
    } else if x > max {
        max
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::PI;

    #[test]
    fn wrap_pi_brings_a_large_angle_into_range() {
        for turns in [-1000.0f32, -3.5, -1.0, 0.5, 1.0, 7.0, 1000.0] {
            let wrapped = wrap_pi(turns * TAU + 1.0);
            assert!(
                (-PI..=PI).contains(&wrapped),
                "wrap_pi({turns} turns + 1) = {wrapped}"
            );
        }
    }

    #[test]
    fn wrap_tau_never_returns_the_upper_bound() {
        for x in [-1e-30f32, -1e-38, -f32::MIN_POSITIVE, -1e-7, -0.0] {
            let wrapped = wrap_tau(x);
            assert!((0.0..TAU).contains(&wrapped), "wrap_tau({x}) = {wrapped}");
        }
    }

    #[test]
    fn wrap_pi_of_a_non_finite_angle_is_nan() {
        assert!(wrap_pi(f32::INFINITY).is_nan());
        assert!(wrap_pi(f32::NEG_INFINITY).is_nan());
        assert!(wrap_pi(f32::NAN).is_nan());
    }

    #[test]
    fn angle_delta_takes_the_short_way_round() {
        let d = angle_delta(PI - 0.1, -PI + 0.1);
        assert!(d > 0.0 && d < 0.3, "delta was {d}");
    }

    #[test]
    fn lerp_is_exact_at_both_endpoints() {
        let (a, b) = (0.1f32, 0.7f32);
        assert_eq!(lerp(a, b, 0.0).to_bits(), a.to_bits());
        assert_eq!(lerp(a, b, 1.0).to_bits(), b.to_bits());
    }

    #[test]
    fn clamp_passes_nan_through_rather_than_hiding_it() {
        assert!(clamp(f32::NAN, 0.0, 1.0).is_nan());
    }

    #[test]
    fn clamp_with_inverted_bounds_returns_the_lower_one() {
        assert_eq!(clamp(0.5, 1.0, 0.0), 1.0);
    }

    #[test]
    fn powi_of_anything_to_the_zero_is_one() {
        for x in [0.0f32, -0.0, 1.5, -1.5, f32::INFINITY, f32::NAN] {
            assert_eq!(powi(x, 0).to_bits(), 1.0f32.to_bits(), "x = {x}");
        }
    }

    #[test]
    fn powi_squares_and_multiplies_rather_than_multiplying_repeatedly() {
        let x = 1.0000001f32;
        let sq = x * x;
        assert_eq!(powi(x, 4).to_bits(), (sq * sq).to_bits());
        assert_eq!(powi(x, 3).to_bits(), (x * sq).to_bits());
    }

    #[test]
    fn powi_handles_the_most_negative_exponent_without_overflowing() {
        assert_eq!(powi(1.0, i32::MIN), 1.0);
    }
}
