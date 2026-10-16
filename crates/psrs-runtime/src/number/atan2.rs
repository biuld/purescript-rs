//! Four-quadrant inverse tangent of two binary64 values.
//!
//! Moderate ratios call upstream `libm` 0.2.15 inverse tangent. An
//! exponent gap above 60 returns a signed half-pi, and a negative `x` with a
//! gap below -60 contributes zero before the pi adjustment. That cutoff
//! matches official `Math.atan2`. The argument order is `(y, x)`. Either NaN
//! produces NaN. The operation does not trap. NaN payloads are not part of
//! the public contract.

/// Returns the angle from the positive x axis to `(x, y)`, in radians.
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
        return libm::atan(y);
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
        libm::atan((y / x).abs())
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
    fn large_negative_x_ratios_return_a_signed_half_pi() {
        for (y, x, bits) in [
            (0.1, -1e-20, 0x3ff9_21fb_5444_2d18),
            (-0.1, -1e-20, 0xbff9_21fb_5444_2d18),
            (1.0, -1e-20, 0x3ff9_21fb_5444_2d18),
            (f64::MIN_POSITIVE, -1.0, 0x4009_21fb_5444_2d18),
        ] {
            assert_eq!(atan2(y, x).to_bits(), bits, "{y}, {x}");
        }
    }
}
