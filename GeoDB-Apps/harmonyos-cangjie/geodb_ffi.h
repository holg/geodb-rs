/**
 * GeoDB C FFI API for Cangjie
 *
 * This header documents the C API exposed by geodb_ffi for Cangjie FFI.
 * Generated from: crates/geodb-ffi/src/c_api.rs
 *
 * Usage in Cangjie:
 *   foreign func geodb_engine_new(): CPointer<CGeoDbEngine>
 */

#ifndef GEODB_FFI_H
#define GEODB_FFI_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Opaque Types
// ============================================================================

/**
 * Opaque handle to GeoDbEngine (Rust Arc<GeoDbEngine>)
 */
typedef struct CGeoDbEngine CGeoDbEngine;

// ============================================================================
// Data Structures
// ============================================================================

/**
 * City search result
 */
typedef struct {
    char* name;                 // UTF-8 string, must be freed
    char* state;                // UTF-8 string, must be freed
    char* country;              // UTF-8 string, must be freed
    char* iso2;                 // ISO2 country code, must be freed
    double lat;                 // Latitude
    double lng;                 // Longitude
    uint64_t population;        // Population count
    double distance_km;         // Distance in km, or -1.0 if not applicable
    char* translations_json;    // JSON object of translations, must be freed
} CCityResult;

/**
 * Database statistics
 */
typedef struct {
    uint64_t countries;
    uint64_t states;
    uint64_t cities;
} CDbStatsDto;

/**
 * List of city results (owns the data)
 */
typedef struct {
    CCityResult* data;          // Array of results, must be freed
    size_t len;                 // Number of elements
    size_t capacity;            // Capacity (internal)
} CCityResultList;

// ============================================================================
// Core API
// ============================================================================

/**
 * Create a new GeoDbEngine instance.
 *
 * Returns NULL on failure (e.g., out of memory, data corrupt).
 * Caller must call geodb_engine_free when done.
 */
CGeoDbEngine* geodb_engine_new(void);

/**
 * Free a GeoDbEngine instance.
 *
 * Safe to call with NULL.
 */
void geodb_engine_free(CGeoDbEngine* engine);

/**
 * Get database statistics.
 *
 * Returns a struct by value (no need to free).
 */
CDbStatsDto geodb_engine_stats(const CGeoDbEngine* engine);

/**
 * Get number of countries in database.
 */
uint64_t geodb_engine_country_count(const CGeoDbEngine* engine);

// ============================================================================
// Search API
// ============================================================================

/**
 * Find country by ISO2 code (e.g., "DE", "US").
 *
 * Returns NULL if not found.
 * Caller must free with geodb_city_result_free.
 */
CCityResult* geodb_engine_find_country_by_code(
    const CGeoDbEngine* engine,
    const char* code
);

/**
 * Find countries by substring (case-insensitive).
 *
 * Returns a list of results.
 * Caller must free with geodb_city_result_list_free.
 */
CCityResultList geodb_engine_find_countries_by_substring(
    const CGeoDbEngine* engine,
    const char* substr
);

/**
 * Find states by substring (case-insensitive).
 *
 * Returns a list of results.
 * Caller must free with geodb_city_result_list_free.
 */
CCityResultList geodb_engine_find_states_by_substring(
    const CGeoDbEngine* engine,
    const char* substr
);

/**
 * Find cities by substring (case-insensitive).
 *
 * Returns a list of results.
 * Caller must free with geodb_city_result_list_free.
 */
CCityResultList geodb_engine_find_cities_by_substring(
    const CGeoDbEngine* engine,
    const char* substr
);

/**
 * Find N nearest cities to given coordinates.
 *
 * Results include distance_km field.
 * Caller must free with geodb_city_result_list_free.
 */
CCityResultList geodb_engine_find_nearest(
    const CGeoDbEngine* engine,
    double lat,
    double lng,
    uint32_t count
);

/**
 * Find cities within radius (km) of given coordinates.
 *
 * Results include distance_km field.
 * Caller must free with geodb_city_result_list_free.
 */
CCityResultList geodb_engine_find_in_radius(
    const CGeoDbEngine* engine,
    double lat,
    double lng,
    double radius_km
);

/**
 * Smart search across countries, states, and cities.
 *
 * Returns up to 50 best matches.
 * Caller must free with geodb_city_result_list_free.
 */
CCityResultList geodb_engine_smart_search(
    const CGeoDbEngine* engine,
    const char* query
);

/**
 * Get country name translation.
 *
 * Returns NULL if country or translation not found.
 * Caller must free with geodb_string_free.
 */
char* geodb_engine_get_country_translation(
    const CGeoDbEngine* engine,
    const char* iso2,
    const char* lang_code
);

// ============================================================================
// Memory Management
// ============================================================================

/**
 * Free a string returned from the API.
 *
 * Safe to call with NULL.
 */
void geodb_string_free(char* s);

/**
 * Free a single CityResult.
 *
 * Frees all internal strings.
 * Safe to call with NULL.
 */
void geodb_city_result_free(CCityResult* result);

/**
 * Free a CityResultList.
 *
 * Frees all internal strings and the array itself.
 * Pass by value (not pointer).
 */
void geodb_city_result_list_free(CCityResultList list);

#ifdef __cplusplus
}
#endif

#endif // GEODB_FFI_H
