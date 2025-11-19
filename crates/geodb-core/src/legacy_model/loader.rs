use crate::error::{GeoError, Result};
use super::{build_geodb, CountriesRaw, DefaultBackend, GeoDb};
use flate2::read::GzDecoder;
use once_cell::sync::OnceCell;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

// In-process cache for default unfiltered load()
static GEO_DB_CACHE: OnceCell<GeoDb<DefaultBackend>> = OnceCell::new();
/// Upstream dataset URL used by this crate.
///
/// This crate relies on the Countries+States+Cities dataset maintained at:
/// <https://github.com/dr5hn/countries-states-cities-database>
///
/// Specifically, our default data file corresponds to the JSON GZip export:
/// "json/countries%2Bstates%2Bcities.json.gz" in that repository. If you
/// bundle or update data manually, make sure it matches the structure of that
/// file.
///
/// Licensing: the upstream dataset is licensed under CC-BY-4.0. Please ensure
/// proper attribution when using the data.
const DATA_REPO_URL: &str = "https://github.com/dr5hn/countries-states-cities-database/blob/master/json/countries%2Bstates%2Bcities.json.gz";

impl GeoDb<DefaultBackend> {
    /// Default directory where the bundled dataset is stored: `<crate>/data`.
    ///
    /// This resolves to the `data/` folder inside the `geodb-core` crate. The
    /// default JSON dataset and any generated binary caches live here.
    // pub fn default_data_dir() -> PathBuf {
    //     PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
    // }

    /// Default dataset file name used by the loader.
    ///
    /// File is expected to be located under [`Self::default_data_dir`]. When
    /// using [`GeoDb::<DefaultBackend>::load`], this filename is combined with
    /// the directory to form the full path.
    // pub fn default_dataset_filename() -> &'static str {
    //     "countries+states+cities.json.gz"
    // }

    /// Load the default database (unfiltered) from the bundled dataset.
    ///
    /// Uses an in-process cache to avoid re-parsing on subsequent calls within
    /// the same process. Also creates/uses an on-disk binary cache file next to
    /// the JSON dataset for faster future startups.
    // pub fn load() -> Result<Self> {
    //     GEO_DB_CACHE
    //         .get_or_try_init(|| {
    //             let dir = Self::default_data_dir();
    //             let file = Self::default_dataset_filename();
    //             Self::load_from_path(dir.join(file), None)
    //         })
    //         .cloned()
    // }

    /// Load from a custom on-disk dataset path.
    ///
    /// Parameters:
    /// - `json_path`: Path to a `.json.gz` file compatible with the upstream
    ///   dataset structure.
    /// - `iso2_filter`: Optional slice of ISO2 country codes to limit the
    ///   loaded data. Pass `None` or an empty slice to load all countries.
    ///
    /// Creates/reads a binary cache file adjacent to `json_path` whose name is
    /// derived from the dataset filename and filter.
    pub fn load_from_path(
        json_path: impl AsRef<Path>,
        iso2_filter: Option<&[&str]>,
    ) -> Result<Self> {
        let json_path = json_path.as_ref().to_path_buf();
        load_generic(json_path, iso2_filter)
    }

    /// Returns the canonical upstream URL to the dataset this crate relies on.
    ///
    /// Developers embedding or packaging this crate should acknowledge the
    /// upstream source (CC-BY-4.0) and may use this URL to reference or fetch
    /// the default dataset. Note that the crate ships with a copy under
    /// `geodb-core/data/` for out-of-the-box usage.
    // pub fn get_3rd_party_data_url() -> &'static str {
    //     DATA_REPO_URL
    // }

    /// Load a filtered database using the bundled dataset.
    ///
    /// Only countries whose ISO2 code is contained in `iso2` are loaded. An
    /// on-disk binary cache specific to the filter set is maintained next to
    /// the JSON file.
    pub fn load_filtered_by_iso2(iso2: &[&str]) -> Result<Self> {
        let dir = Self::default_data_dir();
        let file = Self::default_dataset_filename();
        Self::load_from_path(dir.join(file), Some(iso2))
    }
}

/// Core logic: dataset + filter → DB
fn load_generic(json_path: PathBuf, iso2_filter: Option<&[&str]>) -> Result<GeoDb<DefaultBackend>> {
    //
    // Derive cache filename from dataset file name
    //
    let dataset_filename = json_path.file_name().unwrap().to_string_lossy().to_string(); // e.g. "countries+states+cities.json.gz"

    let suffix = match iso2_filter {
        None => "ALL".to_string(),
        Some([]) => "ALL".to_string(),
        Some(list) => list.join("_"),
    };

    let cache_filename = format!("{dataset_filename}.{suffix}.bin");

    // cache file is next to the JSON file
    let bin_path = json_path.parent().unwrap().join(cache_filename);

    //
    // 1) Try binary cache
    //
    if let Ok(bytes) = std::fs::read(&bin_path) {
        if let Ok(db) = bincode::deserialize::<GeoDb<DefaultBackend>>(&bytes) {
            return Ok(db);
        }
    }

    //
    // 2) Load JSON .gz
    //
    let raw = load_raw_countries(&json_path)?;

    // 3) Apply filter
    let mut filtered = raw;
    if let Some(filter) = iso2_filter {
        filtered.retain(|c| filter.contains(&c.iso2.as_str()));
    }

    // 4) Build DB
    let db = build_geodb(filtered);

    //
    // 5) Save new cache
    //
    if let Ok(bin) = bincode::serialize(&db) {
        let _ = std::fs::write(&bin_path, bin);
    }

    Ok(db)
}

/// load `countries+states+cities.json.gz`
fn load_raw_countries(json_path: &Path) -> Result<CountriesRaw> {
    let file = File::open(json_path).map_err(|_| {
        GeoError::NotFound(format!(
            "Dataset not found at path: {}",
            json_path.display()
        ))
    })?;
    let gz = GzDecoder::new(file);
    let reader = BufReader::new(gz);
    Ok(serde_json::from_reader(reader)?)
}
