use core::f64::consts::TAU;

#[must_use]
#[inline]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

#[must_use]
#[inline]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

#[must_use]
#[inline]
pub fn tan(x: f64) -> f64 {
    libm::tan(x)
}

#[must_use]
#[inline]
pub fn sin_cos(x: f64) -> (f64, f64) {
    libm::sincos(x)
}

#[must_use]
#[inline]
pub fn asin(x: f64) -> f64 {
    libm::asin(x)
}

#[must_use]
#[inline]
pub fn acos(x: f64) -> f64 {
    libm::acos(x)
}

#[must_use]
#[inline]
pub fn atan(x: f64) -> f64 {
    libm::atan(x)
}

#[must_use]
#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

#[must_use]
#[inline]
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

#[must_use]
#[inline]
pub fn exp2(x: f64) -> f64 {
    libm::exp2(x)
}

#[must_use]
#[inline]
pub fn exp_m1(x: f64) -> f64 {
    libm::expm1(x)
}

#[must_use]
#[inline]
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

#[must_use]
#[inline]
pub fn ln_1p(x: f64) -> f64 {
    libm::log1p(x)
}

#[must_use]
#[inline]
pub fn log2(x: f64) -> f64 {
    libm::log2(x)
}

#[must_use]
#[inline]
pub fn log10(x: f64) -> f64 {
    libm::log10(x)
}

#[must_use]
#[inline]
pub fn log(x: f64, base: f64) -> f64 {
    libm::log(x) / libm::log(base)
}

#[must_use]
#[inline]
pub fn powf(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

#[must_use]
pub fn powi(x: f64, n: i32) -> f64 {
    let mut acc: f64 = 1.0;
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
pub fn cbrt(x: f64) -> f64 {
    libm::cbrt(x)
}

#[must_use]
#[inline]
pub fn hypot(x: f64, y: f64) -> f64 {
    libm::hypot(x, y)
}

#[must_use]
#[inline]
pub fn sinh(x: f64) -> f64 {
    libm::sinh(x)
}

#[must_use]
#[inline]
pub fn cosh(x: f64) -> f64 {
    libm::cosh(x)
}

#[must_use]
#[inline]
pub fn tanh(x: f64) -> f64 {
    libm::tanh(x)
}

#[must_use]
#[inline]
pub fn asinh(x: f64) -> f64 {
    libm::asinh(x)
}

#[must_use]
#[inline]
pub fn acosh(x: f64) -> f64 {
    libm::acosh(x)
}

#[must_use]
#[inline]
pub fn atanh(x: f64) -> f64 {
    libm::atanh(x)
}

#[must_use]
#[inline]
pub fn wrap_pi(radians: f64) -> f64 {
    libm::remainder(radians, TAU)
}

#[must_use]
pub fn wrap_tau(radians: f64) -> f64 {
    let r = libm::fmod(radians, TAU);
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
pub fn angle_delta(from: f64, to: f64) -> f64 {
    wrap_pi(to - from)
}

#[must_use]
#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    (1.0 - t) * a + t * b
}

#[must_use]
#[inline]
pub fn clamp(x: f64, min: f64, max: f64) -> f64 {
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
    use core::f64::consts::PI;

    #[test]
    fn wrap_pi_brings_a_large_angle_into_range() {
        for turns in [-1000.0f64, -3.5, -1.0, 0.5, 1.0, 7.0, 1000.0] {
            let wrapped = wrap_pi(turns * TAU + 1.0);
            assert!(
                (-PI..=PI).contains(&wrapped),
                "wrap_pi({turns} turns + 1) = {wrapped}"
            );
        }
    }

    #[test]
    fn wrap_tau_never_returns_the_upper_bound() {
        for x in [-1e-300f64, -5e-324, -f64::MIN_POSITIVE, -1e-16, -0.0] {
            let wrapped = wrap_tau(x);
            assert!((0.0..TAU).contains(&wrapped), "wrap_tau({x}) = {wrapped}");
        }
    }

    #[test]
    fn wrap_pi_of_a_non_finite_angle_is_nan() {
        assert!(wrap_pi(f64::INFINITY).is_nan());
        assert!(wrap_pi(f64::NEG_INFINITY).is_nan());
        assert!(wrap_pi(f64::NAN).is_nan());
    }

    #[test]
    fn angle_delta_takes_the_short_way_round() {
        let d = angle_delta(PI - 0.1, -PI + 0.1);
        assert!(d > 0.0 && d < 0.3, "delta was {d}");
    }

    #[test]
    fn lerp_is_exact_at_both_endpoints() {
        let (a, b) = (0.1f64, 0.7f64);
        assert_eq!(lerp(a, b, 0.0).to_bits(), a.to_bits());
        assert_eq!(lerp(a, b, 1.0).to_bits(), b.to_bits());
    }

    #[test]
    fn clamp_passes_nan_through_rather_than_hiding_it() {
        assert!(clamp(f64::NAN, 0.0, 1.0).is_nan());
    }

    #[test]
    fn clamp_with_inverted_bounds_returns_the_lower_one() {
        assert_eq!(clamp(0.5, 1.0, 0.0), 1.0);
    }

    #[test]
    fn powi_of_anything_to_the_zero_is_one() {
        for x in [0.0f64, -0.0, 1.5, -1.5, f64::INFINITY, f64::NAN] {
            assert_eq!(powi(x, 0).to_bits(), 1.0f64.to_bits(), "x = {x}");
        }
    }

    #[test]
    fn powi_squares_and_multiplies_rather_than_multiplying_repeatedly() {
        let x = 1.0000001f64;
        let sq = x * x;
        assert_eq!(powi(x, 4).to_bits(), (sq * sq).to_bits());
        assert_eq!(powi(x, 3).to_bits(), (x * sq).to_bits());
    }

    #[test]
    fn powi_handles_the_most_negative_exponent_without_overflowing() {
        assert_eq!(powi(1.0, i32::MIN), 1.0);
    }
}
