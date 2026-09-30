//! f32 math without f64: `libm`'s sinf, cosf, atan2f and friends compute in
//! software double precision, which is very slow on the Cortex-M7 targets
//! with a single-precision FPU (about 20 us per haversine). These are short
//! polynomials in f32 only (Cephes-style), accurate to ~1e-6 for sin/cos and
//! ~2e-5 rad for atan, and `sqrt` is a bit guess and two Newton steps.

use core::f32::consts::{FRAC_2_PI, FRAC_PI_2, PI};

// Cody-Waite split of pi/2.
const PIO2_HI: f32 = 1.570_312_5;
const PIO2_MID: f32 = 4.837_512_969_970_703e-4;
const PIO2_LO: f32 = 7.549_789_948_768_648e-8;

/// sin and cos of `x` (radians) together.
pub fn sin_cos(x: f32) -> (f32, f32) {
    let t = x * FRAC_2_PI;
    let q = (t + if t < 0.0 { -0.5 } else { 0.5 }) as i32; // nearest quadrant
    let qf = q as f32;
    let r = ((x - qf * PIO2_HI) - qf * PIO2_MID) - qf * PIO2_LO;
    let z = r * r;
    let s = ((-1.951_529_6e-4 * z + 8.332_161e-3) * z - 1.666_665_5e-1) * z * r + r;
    let c = ((2.443_315_7e-5 * z - 1.388_731_6e-3) * z + 4.166_664_6e-2) * z * z - 0.5 * z + 1.0;
    match q & 3 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

pub fn sin(x: f32) -> f32 {
    sin_cos(x).0
}

pub fn cos(x: f32) -> f32 {
    sin_cos(x).1
}

/// atan(x), |error| < 2e-5 rad.
fn atan_unit(x: f32) -> f32 {
    // |x| <= 1
    let z = x * x;
    x * (0.999_977_26
        + z * (-0.332_623_47
            + z * (0.193_543_46 + z * (-0.116_432_87 + z * (0.052_653_32 + z * -0.011_721_2)))))
}

/// atan2(y, x).
pub fn atan2(y: f32, x: f32) -> f32 {
    let (ax, ay) = (abs(x), abs(y));
    if ax == 0.0 && ay == 0.0 {
        return 0.0;
    }
    let a = if ay > ax {
        FRAC_PI_2 - atan_unit(ax / ay)
    } else {
        atan_unit(ay / ax)
    };
    let a = if x < 0.0 { PI - a } else { a };
    if y < 0.0 {
        -a
    } else {
        a
    }
}

/// asin(x) for |x| <= 1.
pub fn asin(x: f32) -> f32 {
    let x = x.clamp(-1.0, 1.0);
    atan2(x, sqrt((1.0 - x) * (1.0 + x)))
}

pub fn abs(x: f32) -> f32 {
    if x < 0.0 {
        -x
    } else {
        x
    }
}

/// floor(x) for |x| < 2^31.
pub fn floor(x: f32) -> f32 {
    let i = x as i32;
    (if (i as f32) > x { i - 1 } else { i }) as f32
}

/// Square root: a bit-pattern guess (3.5% off) and two Newton steps
/// (relative error ~2e-7), in f32 only: no library call, about 40 cycles.
pub fn sqrt(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut y = f32::from_bits(0x1fbd_1df5 + (x.to_bits() >> 1));
    y = 0.5 * (y + x / y);
    0.5 * (y + x / y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trig_matches_f64() {
        let mut worst_sin = 0f64;
        let mut worst_atan = 0f64;
        let mut worst_asin = 0f64;
        for i in -4000..=4000 {
            let x = i as f32 * 0.00157; // about +-2 pi
            let (s, c) = sin_cos(x);
            worst_sin = worst_sin.max((f64::from(s) - f64::from(x).sin()).abs());
            worst_sin = worst_sin.max((f64::from(c) - f64::from(x).cos()).abs());
        }
        for i in -200..=200 {
            for j in -200..=200 {
                if i == 0 && j == 0 {
                    continue;
                }
                let (y, x) = (i as f32 * 0.37, j as f32 * 0.91);
                worst_atan = worst_atan
                    .max((f64::from(atan2(y, x)) - f64::from(y).atan2(f64::from(x))).abs());
            }
        }
        for i in -1000..=1000 {
            let x = i as f32 / 1000.0;
            worst_asin = worst_asin.max((f64::from(asin(x)) - f64::from(x).asin()).abs());
        }
        assert!(worst_sin < 2e-6, "sin/cos {worst_sin}");
        assert!(worst_atan < 3e-5, "atan2 {worst_atan}");
        assert!(worst_asin < 3e-4, "asin {worst_asin}");
        // Tiny angles keep their relative precision (short radii).
        let x = 1.0e-5f32;
        assert!((sin(x) - x).abs() / x < 1e-6);
        assert_eq!(
            (floor(-1.5), floor(2.7), floor(3.0), floor(-3.0)),
            (-2.0, 2.0, 3.0, -3.0)
        );
        assert!((sqrt(2.0) - core::f32::consts::SQRT_2).abs() < 1e-6);
        for &v in &[1e-14f32, 3.3e-9, 1e-4, 0.25, 1.0, 7.0, 1234.5] {
            let want = f64::from(v).sqrt();
            assert!((f64::from(sqrt(v)) - want).abs() / want < 5e-7, "sqrt({v})");
        }
        assert_eq!(sqrt(0.0), 0.0);
    }
}
