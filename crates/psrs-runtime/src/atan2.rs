//! Four-quadrant inverse tangent of two binary64 values.
//!
//! The reduction follows the fdlibm cutoff: an exponent gap above 60 uses a
//! signed half-pi, and a negative `x` with an exponent gap below -60 uses a
//! zero before the pi adjustment. Moderate ratios call this crate's inverse
//! tangent, which matches libm `atan` and does not touch the stack pointer.
//! libm 0.2.15's own `atan2` uses a wider gap and differs from official
//! `Math.atan2` by one ulp on some of those large ratios.
//!
//! The argument order is `(y, x)`, matching official `Math.atan2` and
//! `Data.Number.atan2`. Either NaN produces NaN. The operation does not trap.
//! NaN payloads are not part of the public contract.

use super::atan::atan;

/// Returns the angle from the positive x axis to `(x, y)`, in radians.
///
/// The split pi terms are the fdlibm constants. Their extra digits belong to
/// that reduction.
#[allow(clippy::excessive_precision, clippy::approx_constant)]
fn atan2(y: f64, x: f64) -> f64 {
    const PI: f64 = 3.1415926535897931160E+00;
    const PI_LO: f64 = 1.2246467991473531772E-16;
    const PI_OVER_2: f64 = 1.5707963267948965580E+00;

    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    let mut ix = (x.to_bits() >> 32) as u32;
    let lx = x.to_bits() as u32;
    let mut iy = (y.to_bits() >> 32) as u32;
    let ly = y.to_bits() as u32;
    if (ix.wrapping_sub(0x3ff0_0000) | lx) == 0 {
        return atan(y);
    }
    let mut quadrant = ((iy >> 31) & 1) | ((ix >> 30) & 2);
    ix &= 0x7fff_ffff;
    iy &= 0x7fff_ffff;

    if (iy | ly) == 0 {
        return match quadrant {
            0 | 1 => y,
            2 => PI,
            _ => -PI,
        };
    }
    if (ix | lx) == 0 {
        return if quadrant & 1 != 0 {
            -PI / 2.0
        } else {
            PI / 2.0
        };
    }
    if ix == 0x7ff0_0000 {
        if iy == 0x7ff0_0000 {
            return match quadrant {
                0 => PI / 4.0,
                1 => -PI / 4.0,
                2 => 3.0 * PI / 4.0,
                _ => -3.0 * PI / 4.0,
            };
        }
        return match quadrant {
            0 => 0.0,
            1 => -0.0,
            2 => PI,
            _ => -PI,
        };
    }
    if iy == 0x7ff0_0000 {
        return if quadrant & 1 != 0 {
            -PI / 2.0
        } else {
            PI / 2.0
        };
    }
    let exponent_gap = (iy as i32).wrapping_sub(ix as i32) >> 20;
    let reduced = if exponent_gap > 60 {
        quadrant &= 1;
        PI_OVER_2 + 0.5 * PI_LO
    } else if quadrant & 2 != 0 && exponent_gap < -60 {
        0.0
    } else {
        atan((y / x).abs())
    };
    match quadrant {
        0 => reduced,
        1 => -reduced,
        2 => PI - (reduced - PI_LO),
        _ => (reduced - PI_LO) - PI,
    }
}

/// C ABI export of [`atan2`].
///
/// # Safety
/// Every binary64 bit pattern is a valid argument. The function reads no
/// caller memory and retains no pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn number_atan2(y: f64, x: f64) -> f64 {
    atan2(y, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_quadrant_zeros_and_infinities() {
        for (y, x, bits) in [
            (0.0, 1.0, 0),
            (-0.0, 1.0, 0x8000_0000_0000_0000),
            (0.0, -1.0, 0x4009_21fb_5444_2d18),
            (-0.0, -1.0, 0xc009_21fb_5444_2d18),
            (1.0, 0.0, 0x3ff9_21fb_5444_2d18),
            (-1.0, 0.0, 0xbff9_21fb_5444_2d18),
            (1.0, -0.0, 0x3ff9_21fb_5444_2d18),
            (-1.0, -0.0, 0xbff9_21fb_5444_2d18),
            (0.0, 0.0, 0),
            (-0.0, 0.0, 0x8000_0000_0000_0000),
            (0.0, -0.0, 0x4009_21fb_5444_2d18),
            (-0.0, -0.0, 0xc009_21fb_5444_2d18),
            (f64::INFINITY, f64::INFINITY, 0x3fe9_21fb_5444_2d18),
            (f64::INFINITY, f64::NEG_INFINITY, 0x4002_d97c_7f33_21d2),
            (f64::NEG_INFINITY, f64::INFINITY, 0xbfe9_21fb_5444_2d18),
            (f64::NEG_INFINITY, f64::NEG_INFINITY, 0xc002_d97c_7f33_21d2),
            (1.0, f64::INFINITY, 0),
            (-1.0, f64::INFINITY, 0x8000_0000_0000_0000),
            (1.0, f64::NEG_INFINITY, 0x4009_21fb_5444_2d18),
            (-1.0, f64::NEG_INFINITY, 0xc009_21fb_5444_2d18),
            (f64::INFINITY, 1.0, 0x3ff9_21fb_5444_2d18),
            (f64::NEG_INFINITY, -1.0, 0xbff9_21fb_5444_2d18),
            (1.0, 1.0, 0x3fe9_21fb_5444_2d18),
            (1.0, -1.0, 0x4002_d97c_7f33_21d2),
            (-1.0, 1.0, 0xbfe9_21fb_5444_2d18),
            (-1.0, -1.0, 0xc002_d97c_7f33_21d2),
        ] {
            assert_eq!(atan2(y, x).to_bits(), bits, "{y}, {x}");
        }
        assert!(atan2(f64::NAN, 1.0).is_nan());
        assert!(atan2(1.0, f64::NAN).is_nan());
    }

    #[test]
    fn matches_moderate_ratios_with_libm_and_large_ratios_with_half_pi() {
        for (y, x, bits) in [
            (0.1, -1e-20, 0x3ff9_21fb_5444_2d18),
            (-0.1, -1e-20, 0xbff9_21fb_5444_2d18),
            (1.0, -1e-20, 0x3ff9_21fb_5444_2d18),
            (f64::MIN_POSITIVE, -1.0, 0x4009_21fb_5444_2d18),
        ] {
            assert_eq!(atan2(y, x).to_bits(), bits, "{y}, {x}");
        }
        let mut samples = vec![
            (0.0, 1.0),
            (-0.0, -1.0),
            (f64::from_bits(1), 1.0),
            (1.0, f64::from_bits(1)),
            (f64::from_bits(1), f64::from_bits(1)),
            (f64::from_bits(0x000f_ffff_ffff_ffff), 1.0),
            (1e-20, -1.0),
            (1.5, 1.0),
            (3.0, 2.0),
            (2.0, -1.0),
            (-2.0, -1.0),
        ];
        let mut state = 0x5eed_5a17u32;
        for _ in 0..256 {
            let y = f64::from_bits(next_bits(&mut state));
            let x = f64::from_bits(next_bits(&mut state));
            if exponent_gap(y, x).abs() <= 60 && y.is_finite() && x.is_finite() && x != 0.0 {
                samples.push((y, x));
            }
        }
        for (y, x) in samples {
            assert_eq!(
                atan2(y, x).to_bits(),
                libm::atan2(y, x).to_bits(),
                "{y}, {x}"
            );
        }
    }

    fn exponent_gap(y: f64, x: f64) -> i32 {
        let ix = (x.to_bits() >> 32) as u32 & 0x7fff_ffff;
        let iy = (y.to_bits() >> 32) as u32 & 0x7fff_ffff;
        (iy as i32).wrapping_sub(ix as i32) >> 20
    }

    fn next_bits(state: &mut u32) -> u64 {
        let low = step(state);
        let high = step(state);
        u64::from(low) | (u64::from(high) << 32)
    }

    fn step(state: &mut u32) -> u32 {
        *state ^= state.wrapping_shl(13);
        *state ^= state.wrapping_shr(17);
        *state ^= state.wrapping_shl(5);
        *state
    }
}
