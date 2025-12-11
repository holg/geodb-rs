//! C API for Cangjie FFI
//!
//! This provides a pure C interface to geodb_ffi that Cangjie can call directly.
//! UniFFI generates Swift/Kotlin bindings, so we need this separate C layer.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double};
use std::ptr;
use std::sync::Arc;

use crate::{CityResult, DbStatsDto, GeoDbEngine};

// ============================================================================
// C-compatible types
// ============================================================================

/// Opaque pointer to GeoDbEngine
#[repr(C)]
pub struct CGeoDbEngine {
    _private: [u8; 0],
}

/// C-compatible CityResult
#[repr(C)]
pub struct CCityResult {
    pub name: *mut c_char,
    pub state: *mut c_char,
    pub country: *mut c_char,
    pub iso2: *mut c_char,
    pub lat: c_double,
    pub lng: c_double,
    pub population: u64,
    pub distance_km: c_double, // -1.0 if None
    pub translations_json: *mut c_char,
}

/// C-compatible DbStatsDto
#[repr(C)]
pub struct CDbStatsDto {
    pub countries: u64,
    pub states: u64,
    pub cities: u64,
}

/// C-compatible result list
#[repr(C)]
pub struct CCityResultList {
    pub data: *mut CCityResult,
    pub len: usize,
    pub capacity: usize,
}

// ============================================================================
// Conversion helpers
// ============================================================================

impl From<&CityResult> for CCityResult {
    fn from(city: &CityResult) -> Self {
        // Convert translations HashMap to JSON string
        let translations_json =
            serde_json::to_string(&city.translations).unwrap_or_else(|_| "{}".to_string());

        Self {
            name: CString::new(city.name.as_str()).unwrap().into_raw(),
            state: CString::new(city.state.as_str()).unwrap().into_raw(),
            country: CString::new(city.country.as_str()).unwrap().into_raw(),
            iso2: CString::new(city.iso2.as_str()).unwrap().into_raw(),
            lat: city.lat,
            lng: city.lng,
            population: city.population,
            distance_km: city.distance_km.unwrap_or(-1.0),
            translations_json: CString::new(translations_json).unwrap().into_raw(),
        }
    }
}

impl From<&DbStatsDto> for CDbStatsDto {
    fn from(stats: &DbStatsDto) -> Self {
        Self {
            countries: stats.countries,
            states: stats.states,
            cities: stats.cities,
        }
    }
}

// ============================================================================
// C API Functions
// ============================================================================

/// Create a new GeoDbEngine
/// Returns NULL on failure
///
/// # Safety
/// This function is safe to call from C with no arguments.
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_new() -> *mut CGeoDbEngine {
    match GeoDbEngine::new() {
        Ok(engine) => {
            let arc = Arc::new(engine);
            Box::into_raw(Box::new(arc)) as *mut CGeoDbEngine
        }
        Err(_) => ptr::null_mut(),
    }
}

/// Free a GeoDbEngine
///
/// # Safety
/// - `engine` must be a valid pointer returned from `geodb_engine_new`
/// - `engine` must not be used after calling this function
/// - This function must only be called once per engine
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_free(engine: *mut CGeoDbEngine) {
    if !engine.is_null() {
        let _ = Box::from_raw(engine as *mut Arc<GeoDbEngine>);
    }
}

/// Get database statistics
///
/// # Safety
/// `engine` must be a valid pointer to a GeoDbEngine instance
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_stats(engine: *const CGeoDbEngine) -> CDbStatsDto {
    let engine = &*(engine as *const Arc<GeoDbEngine>);
    let stats = engine.stats();
    CDbStatsDto::from(&stats)
}

/// Get country count
///
/// # Safety
/// `engine` must be a valid pointer to a GeoDbEngine instance
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_country_count(engine: *const CGeoDbEngine) -> u64 {
    let engine = &*(engine as *const Arc<GeoDbEngine>);
    engine.country_count()
}

/// Find country by ISO2 code
/// Returns NULL if not found
/// Caller must free with geodb_city_result_free
///
/// # Safety
/// - `engine` must be a valid pointer to a GeoDbEngine instance
/// - `code` must be a valid null-terminated C string or NULL
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_find_country_by_code(
    engine: *const CGeoDbEngine,
    code: *const c_char,
) -> *mut CCityResult {
    if code.is_null() {
        return ptr::null_mut();
    }

    let engine = &*(engine as *const Arc<GeoDbEngine>);
    let code_str = CStr::from_ptr(code).to_string_lossy().to_string();

    match engine.find_country_by_code(code_str) {
        Some(city) => Box::into_raw(Box::new(CCityResult::from(&city))),
        None => ptr::null_mut(),
    }
}

/// Find countries by substring
/// Caller must free with geodb_city_result_list_free
///
/// # Safety
/// - `engine` must be a valid pointer to a GeoDbEngine instance
/// - `substr` must be a valid null-terminated C string or NULL
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_find_countries_by_substring(
    engine: *const CGeoDbEngine,
    substr: *const c_char,
) -> CCityResultList {
    if substr.is_null() {
        return CCityResultList {
            data: ptr::null_mut(),
            len: 0,
            capacity: 0,
        };
    }

    let engine = &*(engine as *const Arc<GeoDbEngine>);
    let substr_str = CStr::from_ptr(substr).to_string_lossy().to_string();

    convert_city_vec(engine.find_countries_by_substring(substr_str))
}

/// Find states by substring
///
/// # Safety
/// - `engine` must be a valid pointer to a GeoDbEngine instance
/// - `substr` must be a valid null-terminated C string or NULL
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_find_states_by_substring(
    engine: *const CGeoDbEngine,
    substr: *const c_char,
) -> CCityResultList {
    if substr.is_null() {
        return CCityResultList {
            data: ptr::null_mut(),
            len: 0,
            capacity: 0,
        };
    }

    let engine = &*(engine as *const Arc<GeoDbEngine>);
    let substr_str = CStr::from_ptr(substr).to_string_lossy().to_string();

    convert_city_vec(engine.find_states_by_substring(substr_str))
}

/// Find cities by substring
///
/// # Safety
/// - `engine` must be a valid pointer to a GeoDbEngine instance
/// - `substr` must be a valid null-terminated C string or NULL
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_find_cities_by_substring(
    engine: *const CGeoDbEngine,
    substr: *const c_char,
) -> CCityResultList {
    if substr.is_null() {
        return CCityResultList {
            data: ptr::null_mut(),
            len: 0,
            capacity: 0,
        };
    }

    let engine = &*(engine as *const Arc<GeoDbEngine>);
    let substr_str = CStr::from_ptr(substr).to_string_lossy().to_string();

    convert_city_vec(engine.find_cities_by_substring(substr_str))
}

/// Find nearest cities
///
/// # Safety
/// `engine` must be a valid pointer to a GeoDbEngine instance
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_find_nearest(
    engine: *const CGeoDbEngine,
    lat: c_double,
    lng: c_double,
    count: u32,
) -> CCityResultList {
    let engine = &*(engine as *const Arc<GeoDbEngine>);
    convert_city_vec(engine.find_nearest(lat, lng, count))
}

/// Find cities in radius
///
/// # Safety
/// `engine` must be a valid pointer to a GeoDbEngine instance
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_find_in_radius(
    engine: *const CGeoDbEngine,
    lat: c_double,
    lng: c_double,
    radius_km: c_double,
) -> CCityResultList {
    let engine = &*(engine as *const Arc<GeoDbEngine>);
    convert_city_vec(engine.find_in_radius(lat, lng, radius_km))
}

/// Smart search
///
/// # Safety
/// - `engine` must be a valid pointer to a GeoDbEngine instance
/// - `query` must be a valid null-terminated C string or NULL
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_smart_search(
    engine: *const CGeoDbEngine,
    query: *const c_char,
) -> CCityResultList {
    if query.is_null() {
        return CCityResultList {
            data: ptr::null_mut(),
            len: 0,
            capacity: 0,
        };
    }

    let engine = &*(engine as *const Arc<GeoDbEngine>);
    let query_str = CStr::from_ptr(query).to_string_lossy().to_string();

    convert_city_vec(engine.smart_search(query_str))
}

/// Get country translation
/// Returns NULL if not found
/// Caller must free with geodb_string_free
///
/// # Safety
/// - `engine` must be a valid pointer to a GeoDbEngine instance
/// - `iso2` must be a valid null-terminated C string or NULL
/// - `lang_code` must be a valid null-terminated C string or NULL
#[no_mangle]
pub unsafe extern "C" fn geodb_engine_get_country_translation(
    engine: *const CGeoDbEngine,
    iso2: *const c_char,
    lang_code: *const c_char,
) -> *mut c_char {
    if iso2.is_null() || lang_code.is_null() {
        return ptr::null_mut();
    }

    let engine = &*(engine as *const Arc<GeoDbEngine>);
    let iso2_str = CStr::from_ptr(iso2).to_string_lossy().to_string();
    let lang_str = CStr::from_ptr(lang_code).to_string_lossy().to_string();

    match engine.get_country_translation(iso2_str, lang_str) {
        Some(translation) => CString::new(translation).unwrap().into_raw(),
        None => ptr::null_mut(),
    }
}

/// Free a string returned from FFI
///
/// # Safety
/// - `s` must be a valid pointer returned from a geodb FFI function or NULL
/// - `s` must not be used after calling this function
/// - This function must only be called once per string
#[no_mangle]
pub unsafe extern "C" fn geodb_string_free(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

/// Free a single CityResult
///
/// # Safety
/// - `result` must be a valid pointer returned from a geodb FFI function or NULL
/// - `result` must not be used after calling this function
/// - This function must only be called once per result
#[no_mangle]
pub unsafe extern "C" fn geodb_city_result_free(result: *mut CCityResult) {
    if !result.is_null() {
        let city = Box::from_raw(result);
        geodb_string_free(city.name);
        geodb_string_free(city.state);
        geodb_string_free(city.country);
        geodb_string_free(city.iso2);
        geodb_string_free(city.translations_json);
    }
}

/// Free a CityResultList
///
/// # Safety
/// - `list` must be a valid CCityResultList returned from a geodb FFI function
/// - `list.data` must not be used after calling this function
/// - This function must only be called once per list
#[no_mangle]
pub unsafe extern "C" fn geodb_city_result_list_free(list: CCityResultList) {
    if !list.data.is_null() {
        // Free each city result
        for i in 0..list.len {
            let city = list.data.add(i);
            geodb_string_free((*city).name);
            geodb_string_free((*city).state);
            geodb_string_free((*city).country);
            geodb_string_free((*city).iso2);
            geodb_string_free((*city).translations_json);
        }

        // Free the array itself
        Vec::from_raw_parts(list.data, list.len, list.capacity);
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn convert_city_vec(cities: Vec<CityResult>) -> CCityResultList {
    if cities.is_empty() {
        return CCityResultList {
            data: ptr::null_mut(),
            len: 0,
            capacity: 0,
        };
    }

    let mut c_cities: Vec<CCityResult> = cities.iter().map(CCityResult::from).collect();

    let len = c_cities.len();
    let capacity = c_cities.capacity();
    let data = c_cities.as_mut_ptr();

    std::mem::forget(c_cities); // Don't drop the vec, Cangjie will free it

    CCityResultList {
        data,
        len,
        capacity,
    }
}
