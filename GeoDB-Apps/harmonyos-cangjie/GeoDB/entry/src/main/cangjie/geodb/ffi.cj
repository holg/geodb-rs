// Cangjie FFI bindings for geodb_ffi
// This maps to the C API generated from Rust via UniFFI
//
// Based on: crates/geodb-ffi/src/geodb.udl

package geodb

// ============================================================================
// C Type Declarations (from geodb_ffi.h)
// ============================================================================

// Opaque pointer to GeoDbEngine (Rust Arc<GeoDbEngine>)
public struct CGeoDbEngine {}

// CityResult struct (mirrors the UDL dictionary)
public struct CCityResult {
    public var name: CPointer<UInt8>           // *const char
    public var state: CPointer<UInt8>          // *const char
    public var country: CPointer<UInt8>        // *const char
    public var iso2: CPointer<UInt8>           // *const char
    public var lat: Float64
    public var lng: Float64
    public var population: UInt64
    public var distance_km: Float64  // -1.0 if None
    public var translations_json: CPointer<UInt8>  // JSON string of translations
}

// DbStatsDto struct
public struct CDbStatsDto {
    public var countries: UInt64
    public var states: UInt64
    public var cities: UInt64
}

// Result list (vector wrapper)
public struct CCityResultList {
    public var data: CPointer<CCityResult>
    public var len: UInt64
    public var capacity: UInt64
}

// ============================================================================
// Foreign Function Interface declarations
// ============================================================================

// Constructor
foreign func geodb_engine_new(): CPointer<CGeoDbEngine>

// Destructor
foreign func geodb_engine_free(engine: CPointer<CGeoDbEngine>): Unit

// Stats
foreign func geodb_engine_stats(engine: CPointer<CGeoDbEngine>): CDbStatsDto

// Country count
foreign func geodb_engine_country_count(engine: CPointer<CGeoDbEngine>): UInt64

// Find country by code
foreign func geodb_engine_find_country_by_code(
    engine: CPointer<CGeoDbEngine>,
    code: CPointer<UInt8>
): CPointer<CCityResult>

// Find countries by substring
foreign func geodb_engine_find_countries_by_substring(
    engine: CPointer<CGeoDbEngine>,
    substr: CPointer<UInt8>
): CCityResultList

// Find states by substring
foreign func geodb_engine_find_states_by_substring(
    engine: CPointer<CGeoDbEngine>,
    substr: CPointer<UInt8>
): CCityResultList

// Find cities by substring
foreign func geodb_engine_find_cities_by_substring(
    engine: CPointer<CGeoDbEngine>,
    substr: CPointer<UInt8>
): CCityResultList

// Find nearest
foreign func geodb_engine_find_nearest(
    engine: CPointer<CGeoDbEngine>,
    lat: Float64,
    lng: Float64,
    count: UInt32
): CCityResultList

// Find in radius
foreign func geodb_engine_find_in_radius(
    engine: CPointer<CGeoDbEngine>,
    lat: Float64,
    lng: Float64,
    radius_km: Float64
): CCityResultList

// Smart search
foreign func geodb_engine_smart_search(
    engine: CPointer<CGeoDbEngine>,
    query: CPointer<UInt8>
): CCityResultList

// Get country translation
foreign func geodb_engine_get_country_translation(
    engine: CPointer<CGeoDbEngine>,
    iso2: CPointer<UInt8>,
    lang_code: CPointer<UInt8>
): CPointer<UInt8>

// Free string returned from FFI
foreign func geodb_string_free(s: CPointer<UInt8>): Unit

// Free CityResult
foreign func geodb_city_result_free(result: CPointer<CCityResult>): Unit

// Free result list
foreign func geodb_city_result_list_free(list: CCityResultList): Unit
