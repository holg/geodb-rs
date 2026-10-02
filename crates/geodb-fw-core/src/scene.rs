//! The pictures the globe is drawn from, built and kept up to date by one piece of code.
//!
//! The board, the mirror window (and anything else that draws the board's screen) all call [`Pictures::build`]
//! once, [`Pictures::rebake`] when the layers change and [`Pictures::full`] / [`Pictures::quick`] to draw, so the
//! earth they show is made the same way: the hybrid (or procedural) colours, the relief shading, the half-size
//! copy for a moving globe with the coastline and isohypses baked in. Only the memory differs: the board hands
//! in slices of its SDRAM, the hosts hand in vectors. A change to how the earth is made is a change here, once.

use crate::coast::{self, RasterError};
use crate::relief::{self, Dem};
use crate::render::{Earth, Texture};
use crate::{ui, FwImage};

/// The memory the pictures live in (all RGB565 little-endian bytes unless noted).
pub struct Buffers<'a> {
    /// The clean earth, `EARTH_W` x `EARTH_H` (4 MB).
    pub plain: &'a mut [u8],
    /// The same with the relief shading (4 MB).
    pub shaded: &'a mut [u8],
    /// Half-size copies for a moving globe (1 MB each), the strokes baked in.
    pub plain_mip: &'a mut [u8],
    pub shaded_mip: &'a mut [u8],
    /// The unpacked elevation picture (one byte a cell, up to the image's size).
    pub dem: &'a mut [u8],
    /// [`coast::WORK`] bytes of working memory for the hybrid colours.
    pub work: &'a mut [u8],
    /// Scratch for the rasterizer's crossings (a few hundred KB) and, after that, the shade field (one byte
    /// a cell of the elevation picture); only used by `build`.
    pub scratch: &'a mut [u8],
}

/// The earth pictures and the elevation picture, ready to draw from.
pub struct Pictures<'a> {
    b: Buffers<'a>,
    dem_dims: Option<(usize, usize)>,
    /// The layer bits the half-size copies were baked for ([`ui::bake_signature`]).
    baked: u8,
}

impl<'a> Pictures<'a> {
    /// Makes the pictures from the image. `pet` is called between the long steps (the board feeds its
    /// watchdog with it).
    pub fn build(
        img: &FwImage<'_>,
        b: Buffers<'a>,
        pet: &mut dyn FnMut(),
    ) -> Result<Self, RasterError> {
        let (w, h) = (ui::EARTH_W, ui::EARTH_H);
        // the hybrid (the image's Blue Marble colours under its vector coast), or the procedural colours
        coast::earth(img, w, h, b.plain, b.work, b.scratch)?;
        pet();
        b.shaded.copy_from_slice(b.plain);
        // the relief: the shade field mixed into the colours; the elevation picture stays for the contours
        let mut dem_dims = None;
        if let Some((packed, dw, dh)) = img.elev() {
            if dw * dh <= b.dem.len()
                && dw * dh <= b.scratch.len()
                && relief::unpack(packed, dw, dh, b.dem)
            {
                pet();
                let dem = Dem {
                    w: dw,
                    h: dh,
                    px: &b.dem[..dw * dh],
                };
                let shade = &mut b.scratch[..dw * dh];
                relief::shade_field(&dem, ui::SHADE_EXAGGERATION, shade);
                pet();
                relief::apply_in_place(b.shaded, w, h, shade, dw, dh, ui::SHADE_STRENGTH);
                ui::CONTOURS.store(true, core::sync::atomic::Ordering::Relaxed);
                dem_dims = Some((dw, dh));
            }
        }
        let mut p = Pictures {
            b,
            dem_dims,
            baked: 0,
        };
        p.make_mips(img);
        pet();
        Ok(p)
    }

    /// The half-size copies: halved from the earths, the strokes of the lit layers baked in.
    fn make_mips(&mut self, img: &FwImage<'_>) {
        let (w, h) = (ui::EARTH_W, ui::EARTH_H);
        coast::halve(self.b.shaded, w, h, self.b.shaded_mip);
        coast::halve(self.b.plain, w, h, self.b.plain_mip);
        let dem = self.dem_dims.map(|(dw, dh)| Dem {
            w: dw,
            h: dh,
            px: &self.b.dem[..dw * dh],
        });
        ui::bake_moving_layers(self.b.shaded_mip, w / 2, h / 2, img, dem.as_ref());
        ui::bake_moving_layers(self.b.plain_mip, w / 2, h / 2, img, dem.as_ref());
        self.baked = ui::bake_signature();
    }

    /// Makes the half-size copies again when the layers changed since they were baked; returns whether it did.
    /// A few hundred ms on the board: call it at rest.
    pub fn rebake(&mut self, img: &FwImage<'_>, pet: &mut dyn FnMut()) -> bool {
        if ui::bake_signature() == self.baked {
            return false;
        }
        pet();
        self.make_mips(img);
        pet();
        true
    }

    /// The elevation picture, when the image has one.
    pub fn dem(&self) -> Option<Dem<'_>> {
        self.dem_dims.map(|(w, h)| Dem {
            w,
            h,
            px: &self.b.dem[..w * h],
        })
    }

    /// Draws one frame the way the board does, and so does everything that draws the board's screen: a quick
    /// frame (the globe from the half-size earth, then the parts that follow the view) when `quick_ok`, which the
    /// caller sets while the screen moves and a full frame was drawn before, else a full frame (the scope view has
    /// no quick frame either). Returns whether it was a quick one.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_frame(
        &self,
        fb: &mut crate::render::Fb<'_>,
        img: &FwImage<'_>,
        view: crate::render::View,
        spin: ui::Spin,
        extras: &[ui::Extra],
        quick_ok: bool,
        lut: &mut (crate::render::GlobeLut<'_>, crate::render::GlobeLut<'_>),
    ) -> bool {
        if quick_ok && ui::draw_moving(fb, img, view, spin, &mut lut.1, extras, &self.quick()) {
            return true;
        }
        ui::draw(fb, img, view, spin, Some(&mut lut.0), extras, &self.full());
        false
    }

    /// The earth for a full frame (at rest, and at spin speed without the quick path).
    pub fn full(&self) -> Earth<'_> {
        let tex = |px| Texture {
            w: ui::EARTH_W,
            h: ui::EARTH_H,
            px,
        };
        Earth {
            tex: tex(&*self.b.shaded),
            plain: Some(tex(&*self.b.plain)),
            dem: self.dem(),
        }
    }

    /// The earth for a quick frame (the globe while it moves): half size, strokes baked in.
    pub fn quick(&self) -> Earth<'_> {
        let tex = |px| Texture {
            w: ui::EARTH_W / 2,
            h: ui::EARTH_H / 2,
            px,
        };
        Earth {
            tex: tex(&*self.b.shaded_mip),
            plain: Some(tex(&*self.b.plain_mip)),
            dem: self.dem(),
        }
    }
}
