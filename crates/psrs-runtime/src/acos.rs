//! Inverse cosine of one binary64 value.
//!
//! The pinned `libm` 0.2.15 routine is the fdlibm polynomial. On finite inputs
//! in the closed interval [-1, 1] it matches the official JavaScript
//! `Math.acos` results used by purescript-numbers. Inputs outside that
//! interval, infinities, and NaN produce NaN. The operation does not trap.
//! NaN payloads are not part of the public contract.

/// Returns the inverse cosine in radians.
fn acos(value: f64) -> f64 {
    libm::acos(value)
}

/// C ABI export of [`acos`].
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_acos(value: f64) -> f64 {
    acos(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_domain_boundaries_and_rejects_values_outside_the_interval() {
        for (input, bits) in [
            (1.0, 0),
            (-1.0, 0x400921fb54442d18),
            (0.0, 0x3ff921fb54442d18),
            (-0.0, 0x3ff921fb54442d18),
            (0.5, 0x3ff0c152382d7366),
            (-0.5, 0x4000c152382d7366),
        ] {
            assert_eq!(acos(input).to_bits(), bits, "{input}");
        }
        for input in [2.0, -2.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert!(acos(input).is_nan(), "{input}");
        }
    }
}
