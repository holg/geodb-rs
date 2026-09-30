//! Detail tiles for the globe: NASA GIBS imagery in EPSG:4326 (plain
//! latitude/longitude, like the globe's own textures), 512 px tiles. At
//! level L a tile covers 288 / 2^L degrees; the grid starts at (-180, 90).
//! The app keeps a patch of n x n tiles around the view at the level that
//! matches the screen, over the global texture.

/// A tile service.
#[derive(Debug, Clone, Copy)]
pub struct TileSource {
    /// URL key (`?tiles=`).
    pub key: &'static str,
    pub label: &'static str,
    /// `{z}`, `{row}`, `{col}` (and `{date}`) are replaced.
    pub url: &'static str,
    /// Daily imagery: the URL takes a date (YYYY-MM-DD).
    pub dated: bool,
    /// First day with imagery (daily sources).
    pub first_date: &'static str,
    /// Deepest level (GIBS tile matrix set).
    pub max_level: u8,
    pub credit: &'static str,
}

pub const SOURCES: [TileSource; 5] = [
    TileSource {
        key: "bluemarble",
        label: "Blue Marble, 500 m (NASA GIBS)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/BlueMarble_NextGeneration/default/default/500m/{z}/{row}/{col}.jpeg",
        dated: false,
        first_date: "",
        max_level: 7,
        credit: "NASA Blue Marble Next Generation via NASA GIBS",
    },
    TileSource {
        key: "landsat",
        label: "Landsat, 30 m (NASA GIBS, WELD 2000)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/Landsat_WELD_CorrectedReflectance_TrueColor_Global_Annual/default/2000-12-01/31.25m/{z}/{row}/{col}.jpeg",
        dated: false,
        first_date: "",
        max_level: 11,
        credit: "Landsat WELD global annual 2000 via NASA GIBS",
    },
    TileSource {
        key: "modis",
        label: "MODIS Terra, 250 m, daily (NASA GIBS)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/MODIS_Terra_CorrectedReflectance_TrueColor/default/{date}/250m/{z}/{row}/{col}.jpg",
        dated: true,
        first_date: "2000-02-24",
        max_level: 8,
        credit: "MODIS Terra corrected reflectance via NASA GIBS",
    },
    TileSource {
        key: "modis-aqua",
        label: "MODIS Aqua, 250 m, daily, afternoon pass (NASA GIBS)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/MODIS_Aqua_CorrectedReflectance_TrueColor/default/{date}/250m/{z}/{row}/{col}.jpg",
        dated: true,
        first_date: "2002-07-03",
        max_level: 8,
        credit: "MODIS Aqua corrected reflectance via NASA GIBS",
    },
    TileSource {
        key: "viirs",
        label: "VIIRS SNPP, 375 m, daily (NASA GIBS)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/VIIRS_SNPP_CorrectedReflectance_TrueColor/default/{date}/250m/{z}/{row}/{col}.jpg",
        dated: true,
        first_date: "2015-11-24",
        max_level: 8,
        credit: "VIIRS SNPP corrected reflectance via NASA GIBS",
    },
];

/// The first day with MODIS Terra imagery.
pub const FIRST_DATE: &str = "2000-02-24";

/// Pixels per tile side.
pub const TILE_PX: u32 = 512;
/// Below this level the global 16K texture is as sharp.
pub const MIN_LEVEL: u8 = 6;

/// Degrees one tile covers at `level`.
pub fn tile_span(level: u8) -> f64 {
    288.0 / f64::from(1u32 << level)
}

/// (columns, rows) of the grid at `level`.
pub fn matrix(level: u8) -> (i64, i64) {
    let n = f64::from(1u32 << level);
    ((1.25 * n).ceil() as i64, (0.625 * n).ceil() as i64)
}

/// The level whose pixels are at least as fine as `deg_per_px` on screen,
/// at most `max`; `None` when the global texture is enough.
pub fn level_for(deg_per_px: f64, max: u8) -> Option<u8> {
    if deg_per_px.is_nan() || deg_per_px <= 0.0 {
        return None;
    }
    let l = (288.0 / (f64::from(TILE_PX) * deg_per_px)).log2().ceil();
    (l >= f64::from(MIN_LEVEL)).then(|| (l as u8).min(max))
}

/// An n x n block of tiles at one level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub level: u8,
    /// Top-left tile (the column may run past the grid: it wraps).
    pub row0: i64,
    pub col0: i64,
    pub n: u32,
}

impl Window {
    /// The block with (lat, lon) in its middle.
    pub fn around(lat: f64, lon: f64, level: u8, n: u32) -> Window {
        let span = tile_span(level);
        let row = ((90.0 - lat) / span).floor() as i64;
        let col = ((lon + 180.0) / span).floor() as i64;
        let half = i64::from(n / 2);
        Window {
            level,
            row0: row - half,
            col0: col - half,
            n,
        }
    }

    /// West longitude, north latitude and span (degrees) of the block.
    pub fn bounds(&self) -> (f64, f64, f64) {
        let span = tile_span(self.level);
        (
            -180.0 + self.col0 as f64 * span,
            90.0 - self.row0 as f64 * span,
            span * f64::from(self.n),
        )
    }

    /// Whether (lat, lon) is inside the middle half of the block (no need
    /// to move it yet).
    pub fn centred_on(&self, lat: f64, lon: f64) -> bool {
        let (west, north, span) = self.bounds();
        let dl = (lon - west).rem_euclid(360.0) / span;
        let dv = (north - lat) / span;
        (0.25..=0.75).contains(&dl) && (0.25..=0.75).contains(&dv)
    }

    /// (x, y) in the block and (row, col) in the grid of every tile that
    /// exists (rows past the poles are left out, columns wrap).
    pub fn tiles(&self) -> Vec<(u32, u32, i64, i64)> {
        let (cols, rows) = matrix(self.level);
        let mut out = Vec::new();
        for y in 0..self.n {
            let row = self.row0 + i64::from(y);
            if !(0..rows).contains(&row) {
                continue;
            }
            for x in 0..self.n {
                let col = (self.col0 + i64::from(x)).rem_euclid(cols);
                out.push((x, y, row, col));
            }
        }
        out
    }
}

/// The URL of one tile (`date` only matters for daily sources).
pub fn url(source: &TileSource, level: u8, row: i64, col: i64, date: &str) -> String {
    source
        .url
        .replace("{date}", date)
        .replace("{z}", &level.to_string())
        .replace("{row}", &row.to_string())
        .replace("{col}", &col.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_matches_gibs() {
        // GIBS 4326 matrix sizes (from its capabilities document).
        assert_eq!(matrix(0), (2, 1));
        assert_eq!(matrix(1), (3, 2));
        assert_eq!(matrix(3), (10, 5));
        assert_eq!(matrix(7), (160, 80));
        assert_eq!(matrix(11), (2560, 1280));
        // The tile over Munich at level 11 (fetched and checked by eye).
        let w = Window::around(48.14, 11.58, 11, 1);
        assert_eq!((w.row0, w.col0), (297, 1362));
    }

    #[test]
    fn levels_follow_the_screen() {
        // Coarser than the global 16K texture: no tiles.
        assert_eq!(level_for(0.05, 11), None);
        assert_eq!(level_for(tile_span(6) / 512.0, 11), Some(6));
        assert_eq!(level_for(tile_span(9) / 512.0 * 0.9, 11), Some(10));
        assert_eq!(
            level_for(1e-6, 7),
            Some(7),
            "capped at the source's deepest level"
        );
        assert_eq!(level_for(0.0, 7), None);
    }

    #[test]
    fn windows_wrap_and_stop_at_the_poles() {
        let w = Window::around(10.0, 179.9, 7, 8);
        let tiles = w.tiles();
        assert_eq!(tiles.len(), 64);
        let (cols, _) = matrix(7);
        assert!(
            tiles.iter().any(|t| t.3 == 0) && tiles.iter().any(|t| t.3 == cols - 1),
            "wraps the dateline"
        );
        let (west, north, span) = w.bounds();
        assert!(west < 179.9 && west + span > 179.9 && north > 10.0 && north - span < 10.0);
        assert!(w.centred_on(10.0, 179.9) && !w.centred_on(10.0, -150.0));
        let polar = Window::around(89.0, 0.0, 7, 8);
        assert!(polar.tiles().len() < 64 && polar.tiles().iter().all(|t| t.2 >= 0));
        assert!(url(&SOURCES[1], 11, 297, 1362, "").ends_with("/31.25m/11/297/1362.jpeg"));
        let modis = url(&SOURCES[2], 8, 37, 170, "2026-09-29");
        assert!(
            modis.contains("/default/2026-09-29/250m/8/37/170.jpg"),
            "{modis}"
        );
        let keys: Vec<&str> = SOURCES.iter().map(|s| s.key).collect();
        assert_eq!(
            keys,
            ["bluemarble", "landsat", "modis", "modis-aqua", "viirs"]
        );
        assert!(SOURCES
            .iter()
            .filter(|s| s.dated)
            .all(|s| s.first_date >= FIRST_DATE && s.url.contains("{date}")));
        // Every source loads over the network (see the "off" note in the app).
        assert!(SOURCES.iter().all(|s| s.url.starts_with("https://")));
    }
}
