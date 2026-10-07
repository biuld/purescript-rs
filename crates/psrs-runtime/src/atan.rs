//! Inverse tangent of one binary64 value.
//!
//! The polynomial is the fdlibm routine pinned by libm 0.2.15. On every finite
//! input, and on both infinities, it matches the official JavaScript
//! `Math.atan` results used by purescript-numbers, including the sign of zero.
//! NaN produces NaN. The operation does not trap. NaN payloads are not part of
//! the public contract.
//!
//! The original routine forces an `f32` evaluation on subnormal inputs to raise
//! the host underflow flag. Wasm has no floating-point status flags, and that
//! evaluation writes below the stack pointer without reserving the frame. This
//! copy returns the same bits and does not touch the stack pointer.

/// Returns the inverse tangent in radians.
///
/// The decimal coefficients are copied from libm 0.2.15. Their extra digits
/// and the split pi/2 terms belong to that polynomial.
#[allow(clippy::excessive_precision, clippy::approx_constant)]
fn atan(value: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.63647609000806093515e-01,
        7.85398163397448278999e-01,
        9.82793723247329054082e-01,
        1.57079632679489655800e+00,
    ];
    const ATANLO: [f64; 4] = [
        2.26987774529616870924e-17,
        3.06161699786838301793e-17,
        1.39033110312309984516e-17,
        6.12323399573676603587e-17,
    ];
    const AT: [f64; 11] = [
        3.33333333333329318027e-01,
        -1.99999999998764832476e-01,
        1.42857142725034663711e-01,
        -1.11111104054623557880e-01,
        9.09088713343650656196e-02,
        -7.69187620504482999495e-02,
        6.66107313738753120669e-02,
        -5.83357013379057348645e-02,
        4.97687799461593236017e-02,
        -3.65315727442169155270e-02,
        1.62858201153657823623e-02,
    ];

    let mut x = value;
    let mut ix = (x.to_bits() >> 32) as u32;
    let sign = ix >> 31;
    ix &= 0x7fff_ffff;
    if ix >= 0x4410_0000 {
        if x.is_nan() {
            return x;
        }
        let z = ATANHI[3] + f64::from_bits(0x0380_0000);
        return if sign != 0 { -z } else { z };
    }
    let id = if ix < 0x3fdc_0000 {
        if ix < 0x3e40_0000 {
            return x;
        }
        -1
    } else {
        x = x.abs();
        if ix < 0x3ff3_0000 {
            if ix < 0x3fe6_0000 {
                x = (2. * x - 1.) / (2. + x);
                0
            } else {
                x = (x - 1.) / (x + 1.);
                1
            }
        } else if ix < 0x4003_8000 {
            x = (x - 1.5) / (1. + 1.5 * x);
            2
        } else {
            x = -1. / x;
            3
        }
    };
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id < 0 {
        return x - x * (s1 + s2);
    }
    let z = ATANHI[id as usize] - (x * (s1 + s2) - ATANLO[id as usize] - x);
    if sign != 0 { -z } else { z }
}

/// C ABI export of [`atan`].
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_atan(value: f64) -> f64 {
    atan(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_zero_signs_unit_slopes_and_infinities() {
        for (input, bits) in [
            (0.0, 0),
            (-0.0, 0x8000000000000000),
            (1.0, 0x3fe921fb54442d18),
            (-1.0, 0xbfe921fb54442d18),
            (f64::INFINITY, 0x3ff921fb54442d18),
            (f64::NEG_INFINITY, 0xbff921fb54442d18),
        ] {
            assert_eq!(atan(input).to_bits(), bits, "{input}");
        }
        assert!(atan(f64::NAN).is_nan());
    }

    #[test]
    fn matches_libm_bits_including_subnormals() {
        for input in [
            f64::from_bits(1),
            f64::from_bits(0x000f_ffff_ffff_ffff),
            f64::from_bits(0x0010_0000_0000_0000),
            1e-20,
            -1e-20,
            0.4375,
            -0.4375,
            0.7,
            1.1875,
            2.4375,
            1e20,
            f64::MAX,
            -f64::MAX,
        ] {
            assert_eq!(
                atan(input).to_bits(),
                libm::atan(input).to_bits(),
                "{input}"
            );
        }
    }
}
