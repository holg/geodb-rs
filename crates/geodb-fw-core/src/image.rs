//! The flash image, read in place (no copy, no allocation).
//!
//! Little-endian; every section starts on a 4-byte boundary, offsets count
//! from the start of the image. The 64-byte header:
//!
//! | at | field |
//! |---|---|
//! | 0 | magic `GDFW` |
//! | 4 | version u16, flags u16 |
//! | 8 | cities u32, countries u32, named cities u32 |
//! | 20 | texture width u16, height u16 |
//! | 24 | offsets: geoids, country ids, named index, name offsets, names |
//! | 44 | names length; offsets: countries, country names, texture |
//! | 60 | total length |
//!
//! Sections: `geoids` (cities × u32, sorted), `country ids` (cities × u8),
//! `named index` (named × u32 city indices, ascending), `name offsets`
//! ((named + 1) × u32 into `names`), `names` (ASCII, NUL-terminated),
//! `countries` (countries × 4: ISO2, u16 offset into `country names`),
//! `country names` (ASCII, NUL-terminated), `texture` (RGB565, row-major).

pub const MAGIC: [u8; 4] = *b"GDFW";
pub const VERSION: u16 = 1;
pub const HEADER_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    /// Not a geodb firmware image.
    Magic,
    Version,
    /// A section lies outside the image or is not aligned.
    Layout,
}

/// A parsed image over its bytes (in flash on the board).
#[derive(Clone, Copy)]
pub struct FwImage<'a> {
    bytes: &'a [u8],
    cities: usize,
    countries: usize,
    named: usize,
    tex: (usize, usize),
    off_geoids: usize,
    off_country: usize,
    off_named: usize,
    off_name_off: usize,
    off_names: usize,
    off_countries: usize,
    off_country_names: usize,
    off_texture: usize,
}

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
}

fn u16_at(b: &[u8], at: usize) -> usize {
    u16::from_le_bytes([b[at], b[at + 1]]) as usize
}

impl<'a> FwImage<'a> {
    /// Checks the header and that every section fits.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ImageError> {
        if bytes.len() < HEADER_LEN || bytes[..4] != MAGIC {
            return Err(ImageError::Magic);
        }
        if u16_at(bytes, 4) != VERSION as usize {
            return Err(ImageError::Version);
        }
        let img = FwImage {
            bytes,
            cities: u32_at(bytes, 8),
            countries: u32_at(bytes, 12),
            named: u32_at(bytes, 16),
            tex: (u16_at(bytes, 20), u16_at(bytes, 22)),
            off_geoids: u32_at(bytes, 24),
            off_country: u32_at(bytes, 28),
            off_named: u32_at(bytes, 32),
            off_name_off: u32_at(bytes, 36),
            off_names: u32_at(bytes, 40),
            off_countries: u32_at(bytes, 48),
            off_country_names: u32_at(bytes, 52),
            off_texture: u32_at(bytes, 56),
        };
        let names_len = u32_at(bytes, 44);
        let fits = |off: usize, len: usize| {
            off.is_multiple_of(4)
                && off >= HEADER_LEN
                && off.checked_add(len).is_some_and(|end| end <= bytes.len())
        };
        let ok = u32_at(bytes, 60) == bytes.len()
            && fits(img.off_geoids, img.cities * 4)
            && fits(img.off_country, img.cities)
            && fits(img.off_named, img.named * 4)
            && fits(img.off_name_off, (img.named + 1) * 4)
            && fits(img.off_names, names_len)
            && fits(img.off_countries, img.countries * 4)
            && fits(img.off_country_names, 0)
            && fits(img.off_texture, img.tex.0 * img.tex.1 * 2);
        if ok {
            Ok(img)
        } else {
            Err(ImageError::Layout)
        }
    }

    pub fn len(&self) -> usize {
        self.cities
    }

    /// Size of the image in bytes.
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cities == 0
    }

    pub fn countries(&self) -> usize {
        self.countries
    }

    /// Cities that have a name in the image.
    pub fn named(&self) -> usize {
        self.named
    }

    /// The geoid of city `i` (sorted ascending).
    #[inline]
    pub fn geoid(&self, i: usize) -> u32 {
        u32::from_le_bytes([
            self.bytes[self.off_geoids + 4 * i],
            self.bytes[self.off_geoids + 4 * i + 1],
            self.bytes[self.off_geoids + 4 * i + 2],
            self.bytes[self.off_geoids + 4 * i + 3],
        ])
    }

    /// The country of city `i`.
    pub fn country(&self, i: usize) -> usize {
        usize::from(self.bytes[self.off_country + i])
    }

    /// The ISO 3166 alpha-2 code of a country.
    pub fn country_iso(&self, c: usize) -> &'a str {
        let at = self.off_countries + 4 * c;
        core::str::from_utf8(&self.bytes[at..at + 2]).unwrap_or("??")
    }

    pub fn country_name(&self, c: usize) -> &'a str {
        let off = u16_at(self.bytes, self.off_countries + 4 * c + 2);
        self.cstr(self.off_country_names + off)
    }

    /// The name of city `i`, when the image carries one.
    pub fn name(&self, i: usize) -> Option<&'a str> {
        // Binary search of the named index.
        let (mut lo, mut hi) = (0usize, self.named);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let city = u32_at(self.bytes, self.off_named + 4 * mid);
            if city < i {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo < self.named && u32_at(self.bytes, self.off_named + 4 * lo) == i {
            let off = u32_at(self.bytes, self.off_name_off + 4 * lo);
            Some(self.cstr(self.off_names + off))
        } else {
            None
        }
    }

    /// The city index and name of the `n`-th named city (in city order).
    pub fn named_at(&self, n: usize) -> (usize, &'a str) {
        let city = u32_at(self.bytes, self.off_named + 4 * n);
        let off = u32_at(self.bytes, self.off_name_off + 4 * n);
        (city, self.cstr(self.off_names + off))
    }

    /// The earth picture: width, height and RGB565 little-endian pixels.
    pub fn texture(&self) -> (usize, usize, &'a [u8]) {
        let (w, h) = self.tex;
        (
            w,
            h,
            &self.bytes[self.off_texture..self.off_texture + w * h * 2],
        )
    }

    /// First city whose geoid is >= `g`.
    pub fn lower_bound(&self, g: u32) -> usize {
        let (mut lo, mut hi) = (0usize, self.cities);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.geoid(mid) < g {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// First city whose geoid is > `g`.
    pub fn upper_bound(&self, g: u32) -> usize {
        let (mut lo, mut hi) = (0usize, self.cities);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.geoid(mid) <= g {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    fn cstr(&self, at: usize) -> &'a str {
        let rest = &self.bytes[at.min(self.bytes.len())..];
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        core::str::from_utf8(&rest[..end]).unwrap_or("")
    }
}
