//! The executable ECMAScript binary64 formatter built for the Wasm target.

/// Formats a binary64 value using ECMAScript Number::toString semantics.
///
/// # Safety
/// `output` must point to `capacity` writable bytes in the caller's memory,
/// disjoint from this library's static data and stack. The returned length
/// identifies initialized ASCII bytes; no allocation or retained pointer occurs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_to_string(value: f64, output: *mut u8, capacity: usize) -> usize {
    assert!(capacity >= crate::NUMBER_CAPACITY);
    if value.is_finite() {
        // SAFETY: format64 requires 25 writable bytes and a finite input.
        return unsafe { ryu_js::raw::format64(value, output) };
    }
    let text = if value.is_nan() {
        "NaN"
    } else if value.is_sign_negative() {
        "-Infinity"
    } else {
        "Infinity"
    };
    // SAFETY: the caller owns the destination and the checked capacity fits it.
    unsafe { core::ptr::copy_nonoverlapping(text.as_ptr(), output, text.len()) };
    text.len()
}

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_special_values_and_notation_boundaries() {
        for (value, expected) in [
            (-0.0, "0"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (1e20, "100000000000000000000"),
            (1e21, "1e+21"),
            (f64::from_bits(1), "5e-324"),
            (f64::MAX, "1.7976931348623157e+308"),
        ] {
            let mut output = [0xff; crate::NUMBER_CAPACITY];
            // SAFETY: the buffer owns all 32 bytes for the duration of the call.
            let length = unsafe { number_to_string(value, output.as_mut_ptr(), output.len()) };
            assert_eq!(&output[..length], expected.as_bytes());
            assert!(output[length..].iter().all(|byte| *byte == 0xff));
        }
    }
}
