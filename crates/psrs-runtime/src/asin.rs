//! Inverse sine of one binary64 value.
//!
//! The pinned `libm` 0.2.15 routine is the fdlibm polynomial. On finite inputs
//! in the closed interval [-1, 1] it matches the official JavaScript
//! `Math.asin` results used by purescript-numbers, including the sign of zero.
//! Inputs outside that interval, infinities, and NaN produce NaN. The operation
//! does not trap. NaN payloads are not part of the public contract.

/// Returns the inverse sine in radians.
fn asin(value: f64) -> f64 {
    libm::asin(value)
}

/// C ABI export of [`asin`].
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_asin(value: f64) -> f64 {
    asin(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_domain_boundaries_and_rejects_values_outside_the_interval() {
        for (input, bits) in [
            (0.0, 0),
            (-0.0, 0x8000000000000000),
            (1.0, 0x3ff921fb54442d18),
            (-1.0, 0xbff921fb54442d18),
            (0.5, 0x3fe0c152382d7366),
            (-0.5, 0xbfe0c152382d7366),
        ] {
            assert_eq!(asin(input).to_bits(), bits, "{input}");
        }
        for input in [2.0, -2.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert!(asin(input).is_nan(), "{input}");
        }
    }
}
