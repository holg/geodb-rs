// crates/geodb-core/src/loader/mod.rs
use super::model_impl::GeoDb;
use crate::error::Result;
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};

pub mod binary_load;
pub mod common_io; // Adds load_binary_file() to GeoDb
                   // pub use crate::traits::{DbStats, DefaultBackend};
pub use super::{DbStats, DefaultBackend};
#[cfg(feature = "builder")]
pub mod builder; // Adds load_via_builder() and load_raw_json() to GeoDb
static GEO_DB_CACHE: OnceCell<GeoDb<DefaultBackend>> = OnceCell::new();
pub const DATA_REPO_URL: &str = "https://github.com/dr5hn/countries-states-cities-database/blob/master/json/countries%2Bstates%2Bcities.json.gz";
impl GeoDb<DefaultBackend> {
    pub fn default_data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
    }
    pub fn default_dataset_filename() -> String {
        format!("geodb{}", get_suffix())
    }

    pub fn get_data_url() -> &'static str {
        DATA_REPO_URL
    }
    pub fn default_raw_file() -> PathBuf {
        PathBuf::from("countries+states+cities.json.gz")
    }
    pub fn default_raw_path() -> PathBuf {
        Self::default_data_dir().join(Self::default_raw_file())
    }
    pub fn default_bin_path() -> PathBuf {
        Self::default_data_dir().join(Self::default_dataset_filename())
    }

    pub fn load() -> Result<Self> {
        GEO_DB_CACHE
            .get_or_try_init(|| Self::load_from_path(Self::default_bin_path(), None))
            .cloned()
    }
    /// **Unified Loader:**
    /// Dispatches to the appropriate implementation based on file type and features.
    pub fn load_from_path(path: impl AsRef<Path>, filter: Option<&[&str]>) -> Result<Self> {
        let path = path.as_ref();

        // 1. Explicit Binary File? -> Direct Load (Always available)
        if path.extension().and_then(|s| s.to_str()) == Some("bin") {
            return Self::load_binary_file(path, filter);
        }

        // 2. Source File? -> Logic depends on 'builder' feature
        #[cfg(feature = "builder")]
        {
            Self::load_via_builder(path, filter)
        }

        // 3. Fallback for Non-Builder builds (e.g. WASM)
        #[cfg(not(feature = "builder"))]
        {
            use super::GeoError;
            // Try to find the binary cache anyway, even if we can't parse source
            let cache_path = common_io::get_cache_path(path);
            if cache_path.exists() {
                return Self::load_binary_file(&cache_path, filter);
            }

            Err(GeoError::InvalidData(format!(
                "Cannot load source file {path:?}: 'builder' feature is disabled and no binary cache found at {cache_path:?}."
            )))
        }
    }

    pub fn load_filtered_by_iso2(iso2: &[&str]) -> Result<Self> {
        let dir = Self::default_data_dir();
        let file = Self::default_dataset_filename();
        Self::load_from_path(dir.join(file), Some(iso2))
    }
}

pub fn get_suffix() -> String {
    let mut suffix = String::new();
    if cfg!(feature = "legacy_model") {
        suffix += ".nested";
    } else {
        suffix += ".flat"
    }
    if cfg!(feature = "compact") {
        suffix += ".comp";
    }
    if cfg!(feature = "use_smolstr") {
        suffix += ".smol";
    }
    if cfg!(feature = "search_blobs") {
        suffix += ".blobs";
    }
    suffix += ".bin";
    suffix
}
