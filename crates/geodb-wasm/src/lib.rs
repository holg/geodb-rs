use geodb_core::{City, Country, GeoBackend, GeoDb, PhoneCodeSearch, StandardBackend, State};
use serde::Serialize;
use serde_wasm_bindgen::to_value;
use std::sync::OnceLock;
use wasm_bindgen::prelude::*;
// use serde_json::Value;
use std::ops::Not;
#[cfg(target_arch = "wasm32")]
static EMBEDDED_DB: &[u8] = include_bytes!("../../../data/geodb.standard.bin");

static DB: OnceLock<GeoDb<StandardBackend>> = OnceLock::new();

/* --------------------------------------------------------------------------
   Initialization
-------------------------------------------------------------------------- */

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"Initializing GeoDB WASM module...".into());

    DB.get_or_init(|| {
        web_sys::console::log_1(&"Deserializing embedded DB...".into());
        match bincode::deserialize::<GeoDb<StandardBackend>>(EMBEDDED_DB) {
            Ok(db) => {
                web_sys::console::log_1(
                    &format!("✓ Loaded {} countries", db.countries().len()).into(),
                );
                db
            }
            Err(e) => {
                web_sys::console::error_1(&format!("✗ DB load failed: {}", e).into());
                panic!("Failed to load DB: {}", e);
            }
        }
    });
}

/* --------------------------------------------------------------------------
   Output JS structures
-------------------------------------------------------------------------- */

#[derive(Serialize)]
pub struct JsCountry {
    pub kind: &'static str,
    pub name: String,
    pub emoji: Option<String>,
    pub iso2: String,
    pub iso3: Option<String>,
    pub numeric_code: Option<String>,
    pub phonecode: Option<String>,
    pub capital: Option<String>,
    pub currency: Option<String>,
    pub currency_name: Option<String>,
    pub currency_symbol: Option<String>,
    pub tld: Option<String>,
    pub native_name: Option<String>,

    pub population: Option<i64>,
    pub gdp: Option<i64>,
    pub region: Option<String>,
    pub region_id: Option<i64>,
    pub subregion: Option<String>,
    pub subregion_id: Option<i64>,
    pub nationality: Option<String>,

    pub latitude: Option<f64>,
    pub longitude: Option<f64>,

    pub translations: std::collections::HashMap<String, String>,
}

#[derive(Serialize)]
pub struct JsState {
    pub kind: &'static str,
    pub name: String,
    pub country: String,
    pub emoji: Option<String>,
    pub state_code: Option<String>,
    pub full_code: Option<String>,
}

#[derive(Serialize)]
pub struct JsCity {
    pub kind: &'static str,
    pub name: String,
    pub country: String,
    pub state: String,
    pub emoji: Option<String>,
}

/* --------------------------------------------------------------------------
   Conversion Helpers (no repetition)
-------------------------------------------------------------------------- */

fn country_to_js<B: GeoBackend>(c: &Country<B>) -> JsCountry {
    JsCountry {
        kind: "country",
        name: c.name().into(),
        emoji: c.emoji.as_ref().map(|s| s.as_ref().to_string()),
        iso2: c.iso2().into(),
        iso3: c.iso3.as_ref().map(|s| s.as_ref().to_string()),
        numeric_code: c.numeric_code.as_ref().map(|v| v.as_ref().to_string()),
        phonecode: c
            .phone_code()
            .is_empty()
            .not()
            .then(|| c.phone_code().to_string()),
        capital: c.capital().map(|v| v.to_string()),
        currency: (!c.currency().is_empty()).then(|| c.currency().to_string()),
        currency_name: c.currency_name.as_ref().map(|s| s.as_ref().to_string()),
        currency_symbol: c.currency_symbol.as_ref().map(|s| s.as_ref().to_string()),
        tld: c.tld.as_ref().map(|s| s.as_ref().to_string()),
        native_name: c.native_name.as_ref().map(|s| s.as_ref().to_string()),
        population: c.population(),
        gdp: c.gdp,
        region: (!c.region().is_empty()).then(|| c.region().to_string()),
        region_id: c.region_id,
        subregion: c.subregion.as_ref().map(|s| s.as_ref().to_string()),
        subregion_id: c.subregion_id,
        nationality: c.nationality.as_ref().map(|s| s.as_ref().to_string()),
        latitude: c.latitude.map(B::float_to_f64),
        longitude: c.longitude.map(B::float_to_f64),
        translations: c
            .translations
            .iter()
            .map(|(k, v)| (k.clone(), v.as_ref().to_string()))
            .collect(),
    }
}

fn state_to_js<B: GeoBackend>(country: &Country<B>, s: &State<B>) -> JsState {
    JsState {
        kind: "state",
        name: s.name().to_string(),
        country: country.name().to_string(),
        emoji: country.emoji.as_ref().map(|e| e.as_ref().to_string()),
        state_code: s.state_code.as_ref().map(|v| v.as_ref().to_string()),
        full_code: s.full_code.as_ref().map(|v| v.as_ref().to_string()),
    }
}

fn city_to_js<B: GeoBackend>(country: &Country<B>, state: &State<B>, city: &City<B>) -> JsCity {
    JsCity {
        kind: "city",
        name: city.name().to_string(),
        country: country.name().to_string(),
        state: state.name().to_string(),
        emoji: country.emoji.as_ref().map(|e| e.as_ref().to_string()),
    }
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
        .map(|c| country_to_js(c))
        .collect();

    to_value(&items).unwrap()
}

#[wasm_bindgen]
pub fn search_country_prefix(prefix: &str) -> JsValue {
    let p = prefix.to_ascii_lowercase();
    let items: Vec<_> = DB
        .get()
        .unwrap()
        .countries()
        .iter()
        .filter(|c| c.name().to_ascii_lowercase().starts_with(&p))
        .map(country_to_js)
        .collect();
    to_value(&items).unwrap()
}

/* --------------------------------------------------------------------------
   State Search
-------------------------------------------------------------------------- */

#[wasm_bindgen]
pub fn search_state_substring(substr: &str) -> JsValue {
    let q = substr.to_ascii_lowercase();
    let db = DB.get().unwrap();

    let mut out = Vec::new();

    for c in db.countries() {
        for s in c.states() {
            if s.name().to_ascii_lowercase().contains(&q) {
                out.push(state_to_js(c, s));
            }
        }
    }

    to_value(&out).unwrap()
}

/* --------------------------------------------------------------------------
   City Search
-------------------------------------------------------------------------- */

#[wasm_bindgen]
pub fn search_city_substring(substr: &str) -> JsValue {
    let q = substr.to_ascii_lowercase();
    let db = DB.get().unwrap();

    let mut out = Vec::new();

    for c in db.countries() {
        for s in c.states() {
            for city in s.cities() {
                if city.name().to_ascii_lowercase().contains(&q) {
                    out.push(city_to_js(c, s, city));
                }
            }
        }
    }

    to_value(&out).unwrap()
}

/* --------------------------------------------------------------------------
   Smart Search (country + state + city + phone)
-------------------------------------------------------------------------- */

#[wasm_bindgen]
pub fn smart_search(query: &str) -> JsValue {
    let q = query.trim().to_ascii_lowercase();
    let phone = q.trim_start_matches('+');
    let mut out: Vec<(i32, JsValue)> = Vec::new();

    let db = DB.get().unwrap();

    if q.is_empty() {
        return to_value::<Vec<String>>(&vec![]).unwrap();
    }

    /* --- Countries --- */
    for c in db.countries() {
        let name = c.name().to_ascii_lowercase();

        if c.iso2().eq_ignore_ascii_case(&q) {
            out.push((100, to_value(&country_to_js(c)).unwrap()));
        } else if name == q {
            out.push((90, to_value(&country_to_js(c)).unwrap()));
        } else if name.starts_with(&q) {
            out.push((80, to_value(&country_to_js(c)).unwrap()));
        } else if name.contains(&q) {
            out.push((70, to_value(&country_to_js(c)).unwrap()));
        }
    }

    /* --- States --- */
    for c in db.countries() {
        for s in c.states() {
            let sn = s.name().to_ascii_lowercase();
            if sn.starts_with(&q) {
                out.push((60, to_value(&state_to_js(c, s)).unwrap()));
            } else if sn.contains(&q) {
                out.push((50, to_value(&state_to_js(c, s)).unwrap()));
            }
        }
    }

    /* --- Cities --- */
    for c in db.countries() {
        for s in c.states() {
            for city in s.cities() {
                let cn = city.name().to_ascii_lowercase();
                if cn.starts_with(&q) {
                    out.push((40, to_value(&city_to_js(c, s, city)).unwrap()));
                } else if cn.contains(&q) {
                    out.push((30, to_value(&city_to_js(c, s, city)).unwrap()));
                }
            }
        }
    }

    /* --- Phone Code --- */
    for c in db.find_countries_by_phone_code(phone) {
        out.push((20, to_value(&country_to_js(c)).unwrap()));
    }

    /* Sort by priority */
    out.sort_by(|a, b| b.0.cmp(&a.0));

    /* Extract values */
    let array = js_sys::Array::new();
    for (_, v) in out {
        array.push(&v);
    }
    array.into()
}

#[wasm_bindgen]
pub fn get_stats() -> JsValue {
    let db = DB.get().unwrap();

    let mut state_count = 0usize;
    let mut city_count = 0usize;

    for c in db.countries() {
        state_count += c.states().len();
        for s in c.states() {
            city_count += s.cities().len();
        }
    }

    let stats = serde_json::json!({
        "countries": db.countries().len(),
        "states": state_count,
        "cities": city_count
    });

    serde_wasm_bindgen::to_value(&stats).unwrap()
}
