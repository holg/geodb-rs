//! Detail tiles for the globe, fetched around the view while zoomed in:
//!
//! - NASA GIBS imagery in EPSG:4326 (plain latitude/longitude, like the
//!   globe's own textures), 512 px tiles. At level L a tile covers
//!   288 / 2^L degrees; the grid starts at (-180, 90).
//! - OpenStreetMap in Web Mercator (EPSG:3857), 256 px tiles, 2^z x 2^z;
//!   the shader maps latitude to Mercator (see `shader.wgsl`).
//!
//! The app keeps a patch of n x n tiles around the view at the level that
//! matches the screen, over the global texture.

/// How a source's tile grid maps to the globe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    /// EPSG:4326, NASA GIBS layout.
    Geographic,
    /// EPSG:3857, the slippy map layout (z / x / y).
    Mercator,
}

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
    /// Deepest level.
    pub max_level: u8,
    pub projection: Projection,
    /// Pixels per tile side.
    pub tile_px: u32,
    /// Coarser than this, imagery leaves the globe to the global texture;
    /// a map (`always`) stays at this level instead.
    pub min_level: u8,
    pub always: bool,
    /// Where the tiles come from (said in the online note).
    pub host: &'static str,
    pub credit: &'static str,
    /// Whether a downloaded single file may keep the tiles shown. The
    /// OpenStreetMap tile policy asks for no offline copies.
    pub offline_copy: bool,
}

const GIBS: TileSource = TileSource {
    key: "",
    label: "",
    url: "",
    dated: false,
    first_date: "",
    max_level: 8,
    projection: Projection::Geographic,
    tile_px: 512,
    min_level: 6,
    always: false,
    host: "NASA GIBS (gibs.earthdata.nasa.gov)",
    credit: "",
    offline_copy: true,
};

pub const SOURCES: [TileSource; 6] = [
    TileSource {
        key: "bluemarble",
        label: "Blue Marble, 500 m (NASA GIBS)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/BlueMarble_NextGeneration/default/default/500m/{z}/{row}/{col}.jpeg",
        max_level: 7,
        credit: "NASA Blue Marble Next Generation via NASA GIBS",
        ..GIBS
    },
    TileSource {
        key: "landsat",
        label: "Landsat, 30 m (NASA GIBS, WELD 2000)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/Landsat_WELD_CorrectedReflectance_TrueColor_Global_Annual/default/2000-12-01/31.25m/{z}/{row}/{col}.jpeg",
        max_level: 11,
        credit: "Landsat WELD global annual 2000 via NASA GIBS",
        ..GIBS
    },
    TileSource {
        key: "modis",
        label: "MODIS Terra, 250 m, daily (NASA GIBS)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/MODIS_Terra_CorrectedReflectance_TrueColor/default/{date}/250m/{z}/{row}/{col}.jpg",
        dated: true,
        first_date: "2000-02-24",
        credit: "MODIS Terra corrected reflectance via NASA GIBS",
        ..GIBS
    },
    TileSource {
        key: "modis-aqua",
        label: "MODIS Aqua, 250 m, daily, afternoon pass (NASA GIBS)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/MODIS_Aqua_CorrectedReflectance_TrueColor/default/{date}/250m/{z}/{row}/{col}.jpg",
        dated: true,
        first_date: "2002-07-03",
        credit: "MODIS Aqua corrected reflectance via NASA GIBS",
        ..GIBS
    },
    TileSource {
        key: "viirs",
        label: "VIIRS SNPP, 375 m, daily (NASA GIBS)",
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg4326/best/VIIRS_SNPP_CorrectedReflectance_TrueColor/default/{date}/250m/{z}/{row}/{col}.jpg",
        dated: true,
        first_date: "2015-11-24",
        credit: "VIIRS SNPP corrected reflectance via NASA GIBS",
        ..GIBS
    },
    TileSource {
        key: "osm",
        label: "OpenStreetMap, street map (openstreetmap.org)",
        url: "https://tile.openstreetmap.org/{z}/{col}/{row}.png",
        dated: false,
        first_date: "",
        max_level: 19,
        projection: Projection::Mercator,
        tile_px: 256,
        min_level: 3,
        always: true,
        host: "the OpenStreetMap tile servers (tile.openstreetmap.org)",
        credit: "© OpenStreetMap contributors",
        offline_copy: false,
    },
];

/// The first day with MODIS Terra imagery.
pub const FIRST_DATE: &str = "2000-02-24";

/// Below this level the global 16K texture is as sharp (GIBS).
pub const MIN_LEVEL: u8 = 6;

/// Web Mercator reaches this far north and south.
pub const MERCATOR_MAX_LAT: f64 = 85.051_128_779_806_59;

/// Latitude (degrees) on the Mercator grid: 0 at the top, 1 at the bottom.
pub fn mercator_v(lat: f64) -> f64 {
    let phi = lat.clamp(-MERCATOR_MAX_LAT, MERCATOR_MAX_LAT).to_radians();
    0.5 - (std::f64::consts::FRAC_PI_4 + phi / 2.0).tan().ln() / (2.0 * std::f64::consts::PI)
}

/// The inverse of [`mercator_v`].
pub fn mercator_lat(v: f64) -> f64 {
    let y = (0.5 - v) * 2.0 * std::f64::consts::PI;
    (2.0 * y.exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees()
}

/// Degrees one GIBS tile covers at `level`.
pub fn tile_span(level: u8) -> f64 {
    288.0 / f64::from(1u32 << level)
}

/// (columns, rows) of the grid at `level`.
pub fn matrix(projection: Projection, level: u8) -> (i64, i64) {
    let n = f64::from(1u32 << level);
    match projection {
        Projection::Geographic => ((1.25 * n).ceil() as i64, (0.625 * n).ceil() as i64),
        Projection::Mercator => (n as i64, n as i64),
    }
}

/// The level whose pixels are at least as fine as `deg_per_px` (degrees of
/// arc per screen pixel) at latitude `lat`, at most the source's deepest;
/// `None` when the global texture is enough (imagery only).
pub fn level_for(source: &TileSource, deg_per_px: f64, lat: f64) -> Option<u8> {
    if deg_per_px.is_nan() || deg_per_px <= 0.0 {
        return None;
    }
    let world = match source.projection {
        Projection::Geographic => 288.0,
        // A Mercator pixel covers cos(lat) of its equator ground.
        Projection::Mercator => 360.0 * lat.to_radians().cos().max(0.01),
    };
    let l = (world / (f64::from(source.tile_px) * deg_per_px))
        .log2()
        .ceil();
    if l < f64::from(source.min_level) {
        return source.always.then_some(source.min_level);
    }
    Some((l as u8).min(source.max_level))
}

/// An n x n block of tiles at one level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub projection: Projection,
    pub level: u8,
    /// Top-left tile (the column may run past the grid: it wraps).
    pub row0: i64,
    pub col0: i64,
    pub n: u32,
    /// Pixels per tile side.
    pub px: u32,
}

impl Window {
    /// The block of `source` tiles with (lat, lon) in its middle.
    pub fn around(source: &TileSource, lat: f64, lon: f64, level: u8, n: u32) -> Window {
        let (cols, rows) = matrix(source.projection, level);
        let (row, col) = match source.projection {
            Projection::Geographic => {
                let span = tile_span(level);
                (
                    ((90.0 - lat) / span).floor(),
                    ((lon + 180.0) / span).floor(),
                )
            }
            Projection::Mercator => (
                (mercator_v(lat) * rows as f64).floor(),
                ((lon + 180.0) / 360.0 * cols as f64).floor(),
            ),
        };
        let half = i64::from(n / 2);
        // A grid no larger than the block (a map far out): all of it.
        let whole = rows <= i64::from(n) && cols <= i64::from(n);
        Window {
            projection: source.projection,
            level,
            row0: if whole { 0 } else { row as i64 - half },
            col0: if whole { 0 } else { col as i64 - half },
            n,
            px: source.tile_px,
        }
    }

    /// Longitude span of one tile (degrees).
    fn tile_lon(&self) -> f64 {
        match self.projection {
            Projection::Geographic => tile_span(self.level),
            Projection::Mercator => 360.0 / f64::from(1u32 << self.level),
        }
    }

    /// West longitude, north latitude and longitude span (degrees) of the
    /// block.
    pub fn bounds(&self) -> (f64, f64, f64) {
        let lon = self.tile_lon();
        let west = -180.0 + self.col0 as f64 * lon;
        let north = match self.projection {
            Projection::Geographic => 90.0 - self.row0 as f64 * lon,
            Projection::Mercator => mercator_lat(self.row0 as f64 / f64::from(1u32 << self.level)),
        };
        (west, north, lon * f64::from(self.n))
    }

    /// The shader's `detail` uniform: x west longitude, y north latitude
    /// (degrees), z span (geographic: degrees; Mercator: of the grid, 1 =
    /// the world), w 1 = geographic, 2 = Mercator.
    pub fn uniform(&self) -> [f32; 4] {
        let (west, north, span) = self.bounds();
        match self.projection {
            Projection::Geographic => [west as f32, north as f32, span as f32, 1.0],
            Projection::Mercator => [west as f32, north as f32, (span / 360.0) as f32, 2.0],
        }
    }

    /// Whether (lat, lon) is inside the middle half of the block (no need
    /// to move it yet).
    pub fn centred_on(&self, lat: f64, lon: f64) -> bool {
        let (west, north, span) = self.bounds();
        let (cols, rows) = matrix(self.projection, self.level);
        if rows <= i64::from(self.n) && cols <= i64::from(self.n) {
            return true; // the whole grid
        }
        let dl = (lon - west).rem_euclid(360.0) / span;
        let dv = match self.projection {
            Projection::Geographic => (north - lat) / span,
            Projection::Mercator => {
                (mercator_v(lat) * rows as f64 - self.row0 as f64) / f64::from(self.n)
            }
        };
        (0.25..=0.75).contains(&dl) && (0.25..=0.75).contains(&dv)
    }

    /// Ground metres per tile pixel at latitude `lat`.
    pub fn metres_per_px(&self, lat: f64) -> f64 {
        let m = self.tile_lon() / f64::from(self.px) * 111_320.0;
        match self.projection {
            Projection::Geographic => m,
            Projection::Mercator => m * lat.to_radians().cos(),
        }
    }

    /// Texture side in pixels.
    pub fn size_px(&self) -> u32 {
        self.n * self.px
    }

    /// (x, y) in the block and (row, col) in the grid of every tile that
    /// exists (rows past the poles are left out, columns wrap).
    pub fn tiles(&self) -> Vec<(u32, u32, i64, i64)> {
        let (cols, rows) = matrix(self.projection, self.level);
        let mut out = Vec::new();
        for y in 0..self.n {
            let row = self.row0 + i64::from(y);
            if !(0..rows).contains(&row) {
                continue;
            }
            // Each column once, even when the grid is narrower than the block.
            for x in 0..self.n.min(cols as u32) {
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

    fn source(key: &str) -> &'static TileSource {
        SOURCES.iter().find(|s| s.key == key).unwrap()
    }

    #[test]
    fn grid_matches_gibs() {
        // GIBS 4326 matrix sizes (from its capabilities document).
        let g = Projection::Geographic;
        assert_eq!(matrix(g, 0), (2, 1));
        assert_eq!(matrix(g, 1), (3, 2));
        assert_eq!(matrix(g, 3), (10, 5));
        assert_eq!(matrix(g, 7), (160, 80));
        assert_eq!(matrix(g, 11), (2560, 1280));
        // The tile over Munich at level 11 (fetched and checked by eye).
        let w = Window::around(source("landsat"), 48.14, 11.58, 11, 1);
        assert_eq!((w.row0, w.col0), (297, 1362));
    }

    #[test]
    fn grid_matches_osm() {
        let osm = source("osm");
        // Munich, Marienplatz: the slippy map tile z16 / x 34875 / y 22743.
        let w = Window::around(osm, 48.1374, 11.5755, 16, 1);
        assert_eq!((w.col0, w.row0), (34875, 22743));
        assert_eq!(
            url(osm, 16, w.row0, w.col0, ""),
            "https://tile.openstreetmap.org/16/34875/22743.png"
        );
        for lat in [-80.0, -33.9, 0.0, 48.1374, 85.0] {
            assert!((mercator_lat(mercator_v(lat)) - lat).abs() < 1e-9);
        }
        assert!(mercator_v(MERCATOR_MAX_LAT).abs() < 1e-12);
        // The block's north edge is the top of its first row.
        let w = Window::around(osm, 48.1374, 11.5755, 16, 8);
        let (west, north, span) = w.bounds();
        assert!((mercator_v(north) * 65536.0 - w.row0 as f64).abs() < 1e-6);
        assert!(west < 11.5755 && west + span > 11.5755 && north > 48.1374);
        assert!(w.centred_on(48.1374, 11.5755));
        assert_eq!(w.uniform()[3], 2.0);
        assert!(
            (w.metres_per_px(48.1374) - 1.595).abs() < 0.01,
            "{}",
            w.metres_per_px(48.1374)
        );
        // Far out the whole map, each tile once.
        let world = Window::around(osm, 10.0, 100.0, 3, 8);
        assert_eq!((world.row0, world.col0), (0, 0));
        assert_eq!(world.tiles().len(), 64);
        assert!(world.centred_on(-60.0, -170.0));
        let tiny = Window::around(osm, 10.0, 100.0, 1, 8);
        assert_eq!(tiny.tiles().len(), 4);
    }

    #[test]
    fn levels_follow_the_screen() {
        let (landsat, bm) = (source("landsat"), source("bluemarble"));
        // Coarser than the global 16K texture: no tiles.
        assert_eq!(level_for(landsat, 0.05, 0.0), None);
        assert_eq!(level_for(landsat, tile_span(6) / 512.0, 0.0), Some(6));
        assert_eq!(
            level_for(landsat, tile_span(9) / 512.0 * 0.9, 0.0),
            Some(10)
        );
        assert_eq!(
            level_for(bm, 1e-6, 0.0),
            Some(7),
            "capped at the source's deepest level"
        );
        assert_eq!(level_for(landsat, 0.0, 0.0), None);
        // The map is always there, and finer at high latitude for the
        // same screen resolution (Mercator stretches).
        let osm = source("osm");
        assert_eq!(level_for(osm, 1.0, 0.0), Some(3));
        let dpp = 360.0 / (256.0 * 65536.0);
        assert_eq!(level_for(osm, dpp, 0.0), Some(16));
        assert_eq!(level_for(osm, dpp, 60.0), Some(15));
        assert_eq!(level_for(osm, 1e-9, 0.0), Some(19));
    }

    #[test]
    fn windows_wrap_and_stop_at_the_poles() {
        let modis = source("modis");
        let w = Window::around(modis, 10.0, 179.9, 7, 8);
        let tiles = w.tiles();
        assert_eq!(tiles.len(), 64);
        let (cols, _) = matrix(Projection::Geographic, 7);
        assert!(
            tiles.iter().any(|t| t.3 == 0) && tiles.iter().any(|t| t.3 == cols - 1),
            "wraps the dateline"
        );
        let (west, north, span) = w.bounds();
        assert!(west < 179.9 && west + span > 179.9 && north > 10.0 && north - span < 10.0);
        assert!(w.centred_on(10.0, 179.9) && !w.centred_on(10.0, -150.0));
        let polar = Window::around(modis, 89.0, 0.0, 7, 8);
        assert!(polar.tiles().len() < 64 && polar.tiles().iter().all(|t| t.2 >= 0));
        assert!(url(source("landsat"), 11, 297, 1362, "").ends_with("/31.25m/11/297/1362.jpeg"));
        let day = url(modis, 8, 37, 170, "2026-09-29");
        assert!(
            day.contains("/default/2026-09-29/250m/8/37/170.jpg"),
            "{day}"
        );
        let keys: Vec<&str> = SOURCES.iter().map(|s| s.key).collect();
        assert_eq!(
            keys,
            [
                "bluemarble",
                "landsat",
                "modis",
                "modis-aqua",
                "viirs",
                "osm"
            ]
        );
        assert!(SOURCES
            .iter()
            .filter(|s| s.dated)
            .all(|s| s.first_date >= FIRST_DATE && s.url.contains("{date}")));
        // Every source loads over the network (see the "off" note in the app).
        assert!(SOURCES.iter().all(|s| s.url.starts_with("https://")));
        // Only the map asks for no offline copies.
        assert!(SOURCES.iter().all(|s| s.offline_copy == (s.key != "osm")));
    }
}
