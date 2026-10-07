//! Allocation-free conversion of a complete ASCII decimal token to binary64.

/// Converts a complete signed decimal token, with an optional decimal exponent.
/// Invalid tokens return NaN. Prefix recognition and callback behavior belong
/// to the target library. The pinned Rust parser rounds to the nearest binary64.
///
/// # Safety
/// `input` must identify `length` readable bytes in the caller's memory for the
/// duration of the call. No pointer is retained and no allocation occurs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_from_decimal(input: *const u8, length: usize) -> f64 {
    // SAFETY: the caller provides a readable buffer for this call.
    let bytes = unsafe { core::slice::from_raw_parts(input, length) };
    if bytes
        .iter()
        .any(|byte| !matches!(byte, b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return f64::NAN;
    }
    // SAFETY: every accepted byte is ASCII.
    let token = unsafe { core::str::from_utf8_unchecked(bytes) };
    token.parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(token: &str) -> f64 {
        // SAFETY: the string owns the readable input for the call.
        unsafe { number_from_decimal(token.as_ptr(), token.len()) }
    }

    #[test]
    fn rounds_decimal_boundaries_and_preserves_negative_zero() {
        for (token, expected) in [
            ("42.5", 42.5),
            ("-0", -0.0),
            ("-1e-9999", -0.0),
            ("1e309", f64::INFINITY),
            ("5e-324", f64::from_bits(1)),
            ("2.4703282292062327e-324", 0.0),
            ("2.4703282292062328e-324", f64::from_bits(1)),
            ("9007199254740993", 9007199254740992.0),
            (
                "1.00000000000000011102230246251565404236316680908203125",
                1.0,
            ),
            (
                "1.000000000000000111022302462515654042363166809082031251",
                f64::from_bits(1.0_f64.to_bits() + 1),
            ),
        ] {
            assert_eq!(parse(token).to_bits(), expected.to_bits(), "{token}");
        }
    }

    #[test]
    fn rejects_partial_nondecimal_and_non_ascii_tokens() {
        for token in [
            "", "+", ".", "1e", "1e+", "42.5tail", " 42", "42 ", "0x2a", "inf", "Infinity", "NaN",
            "１",
        ] {
            assert!(parse(token).is_nan(), "{token}");
        }
    }
}
