//! A software globe into an RGB565 framebuffer, and the few drawing
//! primitives the firmware UI needs. Everything is f32 and uses `libm`, so
//! the host preview and the board draw the same pixels.

use crate::geo::DEG_TO_RAD;
use libm::{asinf, atan2f, cosf, floorf, sinf, sqrtf};

/// Row-major RGB565 pixels (native u16).
pub struct Fb<'a> {
    pub px: &'a mut [u16],
    pub w: usize,
    pub h: usize,
}

/// The earth picture in the image: RGB565 little-endian bytes.
#[derive(Clone, Copy)]
pub struct Texture<'a> {
    pub w: usize,
    pub h: usize,
    pub px: &'a [u8],
}

/// The point (degrees) at the centre of the globe, and how far in: zoom 1
/// shows the whole hemisphere, zoom k the cap of angular radius asin(1 / k).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub lat: f32,
    pub lon: f32,
    pub zoom: f32,
}

impl View {
    pub const fn new(lat: f32, lon: f32) -> View {
        View {
            lat,
            lon,
            zoom: 1.0,
        }
    }

    /// The angular radius (radians) of what fits in the disc.
    pub fn span(&self) -> f32 {
        asinf((1.0 / self.zoom.max(1.0)).min(1.0))
    }
}

pub const fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xf8) << 8) | ((g as u16 & 0xfc) << 3) | (b as u16 >> 3)
}

impl Fb<'_> {
    pub fn fill(&mut self, color: u16) {
        self.px.fill(color);
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, color: u16) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = color;
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u16) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, color);
            }
        }
    }
}

fn unpack(c: u16) -> [f32; 3] {
    [
        f32::from((c >> 11) & 0x1f) * (255.0 / 31.0),
        f32::from((c >> 5) & 0x3f) * (255.0 / 63.0),
        f32::from(c & 0x1f) * (255.0 / 31.0),
    ]
}

impl Texture<'_> {
    fn texel(&self, x: isize, y: isize) -> [f32; 3] {
        let x = x.rem_euclid(self.w as isize) as usize;
        let y = y.clamp(0, self.h as isize - 1) as usize;
        let at = (y * self.w + x) * 2;
        unpack(u16::from_le_bytes([self.px[at], self.px[at + 1]]))
    }

    /// Bilinear, wrapped in longitude, at (lat, lon) degrees.
    pub fn sample(&self, lat: f32, lon: f32) -> [f32; 3] {
        let fx = (lon + 180.0) / 360.0 * self.w as f32 - 0.5;
        let fy = (90.0 - lat) / 180.0 * self.h as f32 - 0.5;
        let (x0, y0) = (floorf(fx), floorf(fy));
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let (a, b) = (self.texel(x0, y0), self.texel(x0 + 1, y0));
        let (c, d) = (self.texel(x0, y0 + 1), self.texel(x0 + 1, y0 + 1));
        let mut out = [0f32; 3];
        for k in 0..3 {
            let top = a[k] * (1.0 - tx) + b[k] * tx;
            let bottom = c[k] * (1.0 - tx) + d[k] * tx;
            out[k] = top * (1.0 - ty) + bottom * ty;
        }
        out
    }
}

/// The (lat, lon) degrees at the normalized disc point (nx, ny) (y up), or
/// `None` outside the disc.
pub fn unproject(view: View, nx: f32, ny: f32) -> Option<(f32, f32, f32)> {
    if nx * nx + ny * ny > 1.0 {
        return None;
    }
    // Zoomed in, the disc shows a smaller cap of the sphere.
    let (nx, ny) = (nx / view.zoom.max(1.0), ny / view.zoom.max(1.0));
    let rho2 = nx * nx + ny * ny;
    let z = sqrtf(1.0 - rho2);
    let (s0, c0) = (sinf(view.lat * DEG_TO_RAD), cosf(view.lat * DEG_TO_RAD));
    let lat = asinf((z * s0 + ny * c0).clamp(-1.0, 1.0)) / DEG_TO_RAD;
    let lon = view.lon + atan2f(nx, z * c0 - ny * s0) / DEG_TO_RAD;
    Some((lat, lon, z))
}

/// Where (lat, lon) lands on a globe of pixel `radius` centred at (cx, cy),
/// with its depth (1 at the centre, 0 at the rim); `None` on the far side.
pub fn project(
    view: View,
    radius: f32,
    cx: i32,
    cy: i32,
    lat: f32,
    lon: f32,
) -> Option<(i32, i32, f32)> {
    let (la, dl) = (lat * DEG_TO_RAD, (lon - view.lon) * DEG_TO_RAD);
    let (s0, c0) = (sinf(view.lat * DEG_TO_RAD), cosf(view.lat * DEG_TO_RAD));
    let x = cosf(la) * sinf(dl);
    let y = c0 * sinf(la) - s0 * cosf(la) * cosf(dl);
    let z = s0 * sinf(la) + c0 * cosf(la) * cosf(dl);
    let zoom = view.zoom.max(1.0);
    let (x, y) = (x * zoom, y * zoom);
    if z <= 0.0 || x * x + y * y > 1.0 {
        return None;
    }
    Some((
        cx + floorf(x * radius + 0.5) as i32,
        cy - floorf(y * radius + 0.5) as i32,
        z,
    ))
}

/// Draws the lit globe. The sun is a fixed direction in camera space; a
/// thin blue rim stands for the atmosphere.
pub fn draw_globe(fb: &mut Fb<'_>, cx: i32, cy: i32, radius: i32, view: View, tex: &Texture<'_>) {
    const LIGHT: [f32; 3] = [-0.42, 0.50, 0.76];
    let r = radius as f32;
    for y in (cy - radius).max(0)..=(cy + radius).min(fb.h as i32 - 1) {
        let ny = -((y - cy) as f32) / r;
        for x in (cx - radius).max(0)..=(cx + radius).min(fb.w as i32 - 1) {
            let nx = (x - cx) as f32 / r;
            let Some((lat, lon, z)) = unproject(view, nx, ny) else {
                continue;
            };
            let mut rgb = tex.sample(lat, lon);
            let lambert = (nx * LIGHT[0] + ny * LIGHT[1] + z * LIGHT[2]).max(0.0);
            let shade = 0.30 + 0.85 * lambert;
            let rim = (1.0 - z) * (1.0 - z) * (1.0 - z);
            let glow = [0.30, 0.55, 1.0];
            for k in 0..3 {
                rgb[k] = (rgb[k] * shade + 255.0 * glow[k] * rim * 0.55).clamp(0.0, 255.0);
            }
            fb.set(x, y, rgb565(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8));
        }
    }
}

/// A filled dot of pixel radius `r`.
pub fn dot(fb: &mut Fb<'_>, x: i32, y: i32, r: i32, color: u16) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r + r / 2 {
                fb.set(x + dx, y + dy, color);
            }
        }
    }
}

/// A ring of pixel radius `r`.
pub fn ring(fb: &mut Fb<'_>, x: i32, y: i32, r: i32, color: u16) {
    let n = (r * 8).max(16);
    for k in 0..n {
        let a = k as f32 / n as f32 * core::f32::consts::TAU;
        fb.set(
            x + floorf(cosf(a) * r as f32 + 0.5) as i32,
            y + floorf(sinf(a) * r as f32 + 0.5) as i32,
            color,
        );
    }
}

/// Width in pixels of `s` at `scale`.
pub fn text_width(s: &str, scale: i32) -> i32 {
    s.chars().count() as i32 * 8 * scale
}

/// 8x8 text (ASCII), each font pixel `scale` pixels wide.
pub fn text(fb: &mut Fb<'_>, x: i32, y: i32, s: &str, scale: i32, color: u16) {
    let mut cx = x;
    for ch in s.chars() {
        let glyph = font8x8::legacy::BASIC_LEGACY[if ch.is_ascii() {
            ch as usize
        } else {
            b'?' as usize
        }];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                if bits >> col & 1 == 1 {
                    fb.rect(
                        cx + col * scale,
                        y + row as i32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cx += 8 * scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn project_and_unproject_agree() {
        let view = View::new(35.0, 100.0);
        for &(lat, lon) in &[
            (35.0f32, 100.0f32),
            (48.0, 120.0),
            (10.0, 60.0),
            (60.0, 100.0),
            (-20.0, 130.0),
        ] {
            let (x, y, z) = project(view, 200.0, 0, 0, lat, lon).expect("visible");
            let (la, lo, z2) =
                unproject(view, x as f32 / 200.0, -(y as f32) / 200.0).expect("on the disc");
            assert!(
                (la - lat).abs() < 0.5 && (lo - lon).abs() < 0.6,
                "({lat},{lon}) -> ({la},{lo})"
            );
            assert!((z - z2).abs() < 0.02);
        }
        // Zoomed in, points move out and the rim points drop off the disc.
        let near = View { zoom: 8.0, ..view };
        let (x, y, _) = project(near, 200.0, 0, 0, 36.0, 101.0).expect("in the cap");
        let (la, lo, _) = unproject(near, x as f32 / 200.0, -(y as f32) / 200.0).expect("disc");
        assert!(
            (la - 36.0).abs() < 0.1 && (lo - 101.0).abs() < 0.1,
            "({la},{lo})"
        );
        assert!(project(near, 200.0, 0, 0, 48.0, 120.0).is_none());
        // The far side does not project.
        assert!(project(view, 200.0, 0, 0, -35.0, -80.0).is_none());
    }

    #[test]
    fn a_two_colour_earth_shows_up_on_the_globe() {
        // 8 x 4: western half blue, eastern half green.
        let mut px = vec![];
        for _ in 0..4 {
            for x in 0..8 {
                let c = if x < 4 {
                    rgb565(30, 60, 200)
                } else {
                    rgb565(40, 190, 60)
                };
                px.extend(c.to_le_bytes());
            }
        }
        let tex = Texture {
            w: 8,
            h: 4,
            px: &px,
        };
        let mut buf = vec![0u16; 64 * 64];
        let mut fb = Fb {
            px: &mut buf,
            w: 64,
            h: 64,
        };
        draw_globe(&mut fb, 32, 32, 30, View::new(0.0, 0.0), &tex);
        // Left of the centre looks at the west (blue), right of it at the east.
        let blue = |c: u16| (c & 0x1f) > ((c >> 11) & 0x1f);
        let green = |c: u16| ((c >> 5) & 0x3f) > 2 * (c & 0x1f) + 8;
        assert!(blue(buf[32 * 64 + 20]), "{:x}", buf[32 * 64 + 20]);
        assert!(green(buf[32 * 64 + 44]), "{:x}", buf[32 * 64 + 44]);
        // Outside the disc stays untouched.
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn text_draws_pixels() {
        let mut buf = vec![0u16; 40 * 16];
        let mut fb = Fb {
            px: &mut buf,
            w: 40,
            h: 16,
        };
        text(&mut fb, 0, 0, "Hi", 1, 0xffff);
        assert!(buf.iter().filter(|&&p| p == 0xffff).count() > 15);
        assert_eq!(text_width("Hi", 2), 32);
    }
}
