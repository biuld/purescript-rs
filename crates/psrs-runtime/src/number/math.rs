//! Scalar math exports and JavaScript numeric semantics.
//!
//! Elementary operations call `libm` 0.2.15. Minimum, maximum, sign,
//! remainder, NaN, infinity, and `isNaN` follow the official JavaScript
//! `Math` and `%` contracts. Remainder calls `libm::fmod`. None of these
//! operations trap. NaN payloads are not part of the public contract, except
//! that `isNaN` is true for every NaN.

/// Returns the smaller value. Either NaN produces NaN. If either zero is
/// negative, a zero result is negative zero.
fn minimum(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        return f64::NAN;
    }
    if left == 0.0 && right == 0.0 {
        return f64::from_bits(left.to_bits() | right.to_bits());
    }
    if left < right { left } else { right }
}

/// Returns the larger value. Either NaN produces NaN. A zero result is
/// negative zero only when both zeros are negative.
fn maximum(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        return f64::NAN;
    }
    if left == 0.0 && right == 0.0 {
        return f64::from_bits(left.to_bits() & right.to_bits());
    }
    if left > right { left } else { right }
}

/// JavaScript `%`: the exact remainder, with a zero result taking the
/// dividend's sign. Division by zero or an infinite dividend produces NaN.
fn remainder(dividend: f64, divisor: f64) -> f64 {
    libm::fmod(dividend, divisor)
}

/// JavaScript `Math.pow`. An exponent of zero yields 1, including a NaN base.
/// A NaN exponent, and ±1 raised to an infinity, yield NaN. The C `pow`
/// convention returns 1 for those two shapes.
fn power(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() || (base.abs() == 1.0 && exponent.is_infinite()) {
        return f64::NAN;
    }
    libm::pow(base, exponent)
}

/// `Math.sign`: NaN and both zeros are unchanged. Infinities become ±1.
fn sign(value: f64) -> f64 {
    if value.is_nan() || value == 0.0 {
        value
    } else if value > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// C ABI export of inverse cosine, in radians.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_acos(value: f64) -> f64 {
    libm::acos(value)
}

/// C ABI export of inverse sine, in radians.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_asin(value: f64) -> f64 {
    libm::asin(value)
}

/// C ABI export of inverse tangent, in radians.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_atan(value: f64) -> f64 {
    libm::atan(value)
}

/// C ABI export of sine.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_sin(value: f64) -> f64 {
    libm::sin(value)
}

/// C ABI export of cosine.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_cos(value: f64) -> f64 {
    libm::cos(value)
}

/// C ABI export of tangent.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_tan(value: f64) -> f64 {
    libm::tan(value)
}

/// C ABI export of the base-e exponential.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_exp(value: f64) -> f64 {
    libm::exp(value)
}

/// C ABI export of the natural logarithm.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_log(value: f64) -> f64 {
    libm::log(value)
}

/// C ABI export of exponentiation.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_pow(base: f64, exponent: f64) -> f64 {
    power(base, exponent)
}

/// C ABI export of [`minimum`].
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_min(left: f64, right: f64) -> f64 {
    minimum(left, right)
}

/// C ABI export of [`maximum`].
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_max(left: f64, right: f64) -> f64 {
    maximum(left, right)
}

/// C ABI export of [`sign`].
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_sign(value: f64) -> f64 {
    sign(value)
}

/// C ABI export of [`remainder`].
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_remainder(dividend: f64, divisor: f64) -> f64 {
    remainder(dividend, divisor)
}

/// C ABI export of the NaN predicate. The result is `1` or `0`.
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_is_nan(value: f64) -> i32 {
    i32::from(value.is_nan())
}

/// C ABI export of canonical NaN.
///
/// # Safety
/// The function reads no caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_nan() -> f64 {
    f64::NAN
}

/// C ABI export of positive infinity.
///
/// # Safety
/// The function reads no caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_infinity() -> f64 {
    f64::INFINITY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acos_matches_domain_boundaries_and_rejects_values_outside_the_interval() {
        for (input, bits) in [
            (1.0, 0),
            (-1.0, 0x400921fb54442d18),
            (0.0, 0x3ff921fb54442d18),
            (-0.0, 0x3ff921fb54442d18),
            (0.5, 0x3ff0c152382d7366),
            (-0.5, 0x4000c152382d7366),
        ] {
            assert_eq!(unsafe { number_acos(input) }.to_bits(), bits, "{input}");
        }
        for input in [2.0, -2.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert!(unsafe { number_acos(input) }.is_nan(), "{input}");
        }
    }

    #[test]
    fn asin_matches_domain_boundaries_and_rejects_values_outside_the_interval() {
        for (input, bits) in [
            (0.0, 0),
            (-0.0, 0x8000000000000000),
            (1.0, 0x3ff921fb54442d18),
            (-1.0, 0xbff921fb54442d18),
            (0.5, 0x3fe0c152382d7366),
            (-0.5, 0xbfe0c152382d7366),
        ] {
            assert_eq!(unsafe { number_asin(input) }.to_bits(), bits, "{input}");
        }
        for input in [2.0, -2.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert!(unsafe { number_asin(input) }.is_nan(), "{input}");
        }
    }

    #[test]
    fn atan_matches_zero_signs_unit_slopes_and_infinities() {
        for (input, bits) in [
            (0.0, 0),
            (-0.0, 0x8000000000000000),
            (f64::from_bits(1), 1),
            (f64::from_bits(0x8000_0000_0000_0001), 0x8000_0000_0000_0001),
            (f64::MIN_POSITIVE, f64::MIN_POSITIVE.to_bits()),
            (1.0, 0x3fe921fb54442d18),
            (-1.0, 0xbfe921fb54442d18),
            (f64::INFINITY, 0x3ff921fb54442d18),
            (f64::NEG_INFINITY, 0xbff921fb54442d18),
        ] {
            assert_eq!(unsafe { number_atan(input) }.to_bits(), bits, "{input}");
        }
        assert!(unsafe { number_atan(f64::NAN) }.is_nan());
    }

    #[test]
    fn minimum_and_maximum_propagate_nan_and_select_zero_signs() {
        let negative_zero = -0.0;
        assert_eq!(minimum(1.0, 2.0).to_bits(), 1.0_f64.to_bits());
        assert_eq!(maximum(1.0, 2.0).to_bits(), 2.0_f64.to_bits());
        assert_eq!(
            minimum(negative_zero, 0.0).to_bits(),
            negative_zero.to_bits()
        );
        assert_eq!(
            minimum(0.0, negative_zero).to_bits(),
            negative_zero.to_bits()
        );
        assert_eq!(maximum(negative_zero, 0.0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(
            maximum(negative_zero, negative_zero).to_bits(),
            negative_zero.to_bits()
        );
        assert!(minimum(f64::NAN, 1.0).is_nan());
        assert!(maximum(1.0, f64::NAN).is_nan());
        assert_eq!(
            minimum(f64::NEG_INFINITY, 1.0).to_bits(),
            f64::NEG_INFINITY.to_bits()
        );
        assert_eq!(
            maximum(f64::INFINITY, 1.0).to_bits(),
            f64::INFINITY.to_bits()
        );
    }

    #[test]
    fn remainder_follows_the_dividend_and_rejects_zero_divisors() {
        let negative_zero = -0.0_f64;
        assert_eq!(remainder(5.3, 2.0).to_bits(), 0x3ff4_cccc_cccc_cccc);
        assert_eq!(remainder(-5.3, 2.0).to_bits(), 0xbff4_cccc_cccc_cccc);
        assert_eq!(remainder(5.3, -2.0).to_bits(), 0x3ff4_cccc_cccc_cccc);
        assert_eq!(remainder(-1.0, 1.0).to_bits(), negative_zero.to_bits());
        assert_eq!(remainder(1.0, 0.1).to_bits(), 0x3fb9_9999_9999_9996);
        assert_eq!(remainder(1.0, f64::INFINITY).to_bits(), 1.0_f64.to_bits());
        assert_eq!(
            remainder(negative_zero, 1.0).to_bits(),
            negative_zero.to_bits()
        );
        assert!(remainder(f64::INFINITY, 1.0).is_nan());
        assert!(remainder(1.0, 0.0).is_nan());
        assert!(remainder(1.0, negative_zero).is_nan());
        assert!(remainder(f64::NAN, 1.0).is_nan());
    }

    #[test]
    fn sign_preserves_zeros_and_nan_and_collapses_infinities() {
        let negative_zero = -0.0;
        assert_eq!(sign(0.0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(sign(negative_zero).to_bits(), negative_zero.to_bits());
        assert!(sign(f64::NAN).is_nan());
        assert_eq!(sign(42.0).to_bits(), 1.0_f64.to_bits());
        assert_eq!(sign(-42.0).to_bits(), (-1.0_f64).to_bits());
        assert_eq!(sign(f64::INFINITY).to_bits(), 1.0_f64.to_bits());
        assert_eq!(sign(f64::NEG_INFINITY).to_bits(), (-1.0_f64).to_bits());
    }

    #[test]
    fn constants_power_and_nan_predicate_match_javascript() {
        assert_eq!(unsafe { number_nan() }.to_bits(), 0x7ff8_0000_0000_0000);
        assert_eq!(
            unsafe { number_infinity() }.to_bits(),
            0x7ff0_0000_0000_0000
        );
        assert_eq!(unsafe { number_is_nan(f64::NAN) }, 1);
        assert_eq!(unsafe { number_is_nan(f64::INFINITY) }, 0);
        assert_eq!(unsafe { number_is_nan(0.0) }, 0);
        assert!(power(1.0, f64::NAN).is_nan());
        assert!(power(1.0, f64::INFINITY).is_nan());
        assert!(power(-1.0, f64::NEG_INFINITY).is_nan());
        assert_eq!(power(f64::NAN, 0.0).to_bits(), 1.0_f64.to_bits());
        assert_eq!(power(2.0, 3.0).to_bits(), 8.0_f64.to_bits());
    }
}
