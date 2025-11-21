//! geodb-wasm — WebAssembly bindings for geodb-core
//!
//! This crate exposes a small, ergonomic JS/WASM API built on top of
//! `geodb-core`. It embeds a compact, serialized database in the WASM
//! binary and provides search helpers callable from JavaScript.
//!
//! What it provides
//! ----------------
//! - Automatic initialization on module load (via `#[wasm_bindgen(start)]`)
//! - Basic queries: `get_country_count()`, `get_country_name(iso2)`
//! - Search helpers returning JSON-serializable objects:
//!   - `search_countries_by_phone("+49")`
//!   - `search_state_substring("bavar")`
//!   - `search_city_substring("berlin")`
//!   - `smart_search("us" | "+1" | "berlin" | ...)`
//!
//! Quick start (browser)
//! ---------------------
//! ```javascript
//! import init, { get_country_count, smart_search } from 'geodb-wasm';
//!
//! async function main() {
//!   await init(); // initializes the embedded DB
//!   console.log('Countries:', get_country_count());
//!
//!   const results = smart_search('berlin');
//!   // results is a JSON array of mixed kinds: country/state/city
//!   console.log(results);
//! }
//! main();
//! ```
//!
//! Quick start (Node.js + bundler)
//! -------------------------------
//! ```javascript
//! import init, { search_countries_by_phone } from 'geodb-wasm';
//!
//! (async () => {
//!   await init();
//!   console.log(search_countries_by_phone('+1'));
//! })();
//! ```
//!
//! Notes
//! -----
//! - The WASM build embeds a prebuilt binary database (`geodb.standard.bin`).
//!   If you customize data, rebuild the crate to refresh the embedded bytes.
//! - All exported functions are `wasm_bindgen` bindings and return plain types
//!   or `JsValue` containing JSON-serializable arrays/objects.
//! - See the `dist/` folder for a Trunk-based demo setup.
#[cfg(target_arch = "wasm32")]
use flate2::read::GzDecoder;
#[cfg(target_arch = "wasm32")]
use std::io::Read;

use std::sync::OnceLock;
use wasm_bindgen::prelude::*;

// Core Imports
use geodb_core::prelude::*; // Imports DefaultGeoDb, GeoSearch, etc.
use geodb_core::api::{CityView, CountryView, StateView};
use serde_json::json;
use serde_wasm_bindgen::to_value;

// 1. Embed the Database
// We expect a file named 'geodb.bin' (or whatever standard name you choose) to exist.
// The builder should have created this.
// Use the absolute path calculated by build.rs
#[cfg(all(target_arch = "wasm32", not(docsrs)))]
static EMBEDDED_DB: &[u8] = include_bytes!(env!("GEO_DB_PATH"));

// Docs stub
#[cfg(all(target_arch = "wasm32", docsrs))]
static EMBEDDED_DB: &[u8] = b"";

// 2. Static Instance
static DB: OnceLock<DefaultGeoDb> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"Initializing GeoDB WASM module...".into());

    DB.get_or_init(|| {
        // ... decompression logic ...
        let mut decoder = GzDecoder::new(EMBEDDED_DB);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).expect("Decompression failed");
        web_sys::console::log_1(&DefaultGeoDb::default_dataset_filename().into());
        // ... deserialization ...
        let db: DefaultGeoDb = bincode::deserialize(&decompressed).expect("Deserialize failed");

        // Log stats via Trait
        let stats = db.stats();
        web_sys::console::log_1(&format!("✓ Loaded {} countries", stats.countries).into());

        db
    });
}

/* --------------------------------------------------------------------------
   Basic Queries
-------------------------------------------------------------------------- */

#[wasm_bindgen]
pub fn get_country_count() -> usize {
    DB.get().unwrap().countries().len()
}

#[wasm_bindgen]
pub fn get_country_name(iso2: &str) -> Option<String> {
    DB.get()
        .unwrap()
        .find_country_by_iso2(iso2)
        .map(|c| c.name().to_string())
}

/* --------------------------------------------------------------------------
   Country Search
-------------------------------------------------------------------------- */

#[wasm_bindgen]
pub fn search_countries_by_phone(phone: &str) -> JsValue {
    let code = phone.trim().trim_start_matches('+');
    let db = DB.get().unwrap();

    let items: Vec<_> = db
        .find_countries_by_phone_code(code)
        .iter()
        .map(|c| CountryView(c))
        .collect();

    to_value(&items).unwrap()
}

/* --------------------------------------------------------------------------
   State Search
-------------------------------------------------------------------------- */

#[wasm_bindgen]
pub fn search_state_substring(substr: &str) -> JsValue {
    let db = DB.get().unwrap();

    let out: Vec<_> = db
        .find_states_by_substring(substr)
        .into_iter()
        .map(|(state, country)| StateView { country, state })
        .collect();

    to_value(&out).unwrap()
}

/* --------------------------------------------------------------------------
   City Search
-------------------------------------------------------------------------- */

#[wasm_bindgen]
pub fn search_city_substring(substr: &str) -> JsValue {
    let db = DB.get().unwrap();

    let out: Vec<_> = db
        .find_cities_by_substring(substr)
        .into_iter()
        .map(|(city, state, country)| CityView {
            country,
            state,
            city,
        })
        .collect();

    to_value(&out).unwrap()
}

/* --------------------------------------------------------------------------
   Smart Search (country + state + city + phone)
-------------------------------------------------------------------------- */

#[wasm_bindgen]
pub fn smart_search(query: &str) -> JsValue {
    let db = DB.get().unwrap();
    let hits = db.smart_search(query);

    // Map to JS serializable wrappers while preserving order
    let array = js_sys::Array::new();
    for hit in hits {
        let v = match hit.item {
            SmartItem::Country(c) => to_value(&CountryView(c)).unwrap(),
            SmartItem::State { country, state } => to_value(&StateView { country, state }).unwrap(),
            SmartItem::City {
                country,
                state,
                city,
            } => to_value(&CityView {
                country,
                state,
                city,
            })
            .unwrap(),
        };
        array.push(&v);
    }
    array.into()
}

#[wasm_bindgen]
pub fn get_stats() -> JsValue {
    let db = DB.get().unwrap();
    let stats = db.stats();
    let stats = json!({
        "countries": stats.countries,
        "states": stats.states,
        "cities": stats.cities
    });

    to_value(&stats).unwrap()
}
