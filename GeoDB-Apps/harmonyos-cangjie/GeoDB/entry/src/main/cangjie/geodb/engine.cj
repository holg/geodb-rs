// Safe Cangjie wrapper for GeoDbEngine
// Mirrors the Swift GeoDatabase class
package geodb

import std.collection.*

// ============================================================================
// GeoDbEngine - Main database interface
// ============================================================================

public class GeoDbEngine {
    private let handle: CPointer<CGeoDbEngine>
    private var statsCache: DbStats?

    // Constructor - throws error if initialization fails
    public init() throws {
        unsafe {
            let ptr = geodb_engine_new()
            if (ptr.isNull()) {
                throw Exception("Failed to initialize GeoDb engine")
            }
            this.handle = ptr
        }

        // Cache stats on initialization
        this.statsCache = this.stats()
    }

    // Destructor - free Rust resources
    public func finalize() {
        unsafe {
            if (!this.handle.isNull()) {
                geodb_engine_free(this.handle)
            }
        }
    }

    // Get database statistics
    public func stats(): DbStats {
        if (let cached = this.statsCache) {
            return cached
        }

        unsafe {
            let c_stats = geodb_engine_stats(this.handle)
            return DbStats.fromC(c_stats)
        }
    }

    // Get country count
    public func countryCount(): UInt64 {
        unsafe {
            return geodb_engine_country_count(this.handle)
        }
    }

    // Find country by ISO2 code
    public func findCountryByCode(code: String): CityResult? {
        let cCode = stringToCString(code)
        defer {
            // Free the C string after use
            // geodb_string_free(cCode)
        }

        unsafe {
            let ptr = geodb_engine_find_country_by_code(this.handle, cCode)
            if (ptr.isNull()) {
                return None
            }

            let result = CityResult.fromC(ptr[0])
            geodb_city_result_free(ptr)
            return Some(result)
        }
    }

    // Find countries by substring
    public func findCountries(query: String): ArrayList<CityResult> {
        return findBySubstring(query, geodb_engine_find_countries_by_substring)
    }

    // Find states by substring
    public func findStates(query: String): ArrayList<CityResult> {
        return findBySubstring(query, geodb_engine_find_states_by_substring)
    }

    // Find cities by substring
    public func findCities(query: String): ArrayList<CityResult> {
        return findBySubstring(query, geodb_engine_find_cities_by_substring)
    }

    // Smart search across all entities
    public func smartSearch(query: String): ArrayList<CityResult> {
        let cQuery = stringToCString(query)
        defer {
            // geodb_string_free(cQuery)
        }

        unsafe {
            let list = geodb_engine_smart_search(this.handle, cQuery)
            let results = convertCityResultList(list)
            geodb_city_result_list_free(list)
            return results
        }
    }

    // Find nearest cities to coordinates
    public func findNearest(lat: Float64, lng: Float64, count: UInt32): ArrayList<CityResult> {
        unsafe {
            let list = geodb_engine_find_nearest(this.handle, lat, lng, count)
            let results = convertCityResultList(list)
            geodb_city_result_list_free(list)
            return results
        }
    }

    // Find cities within radius
    public func findInRadius(lat: Float64, lng: Float64, radiusKm: Float64): ArrayList<CityResult> {
        unsafe {
            let list = geodb_engine_find_in_radius(this.handle, lat, lng, radiusKm)
            let results = convertCityResultList(list)
            geodb_city_result_list_free(list)
            return results
        }
    }

    // Get country translation
    public func getCountryTranslation(iso2: String, langCode: String): String? {
        let cIso2 = stringToCString(iso2)
        let cLang = stringToCString(langCode)
        defer {
            // geodb_string_free(cIso2)
            // geodb_string_free(cLang)
        }

        unsafe {
            let ptr = geodb_engine_get_country_translation(this.handle, cIso2, cLang)
            if (ptr.isNull()) {
                return None
            }

            let result = cStringToString(ptr)
            geodb_string_free(ptr)
            return Some(result)
        }
    }

    // ========================================================================
    // Private helper functions
    // ========================================================================

    private func findBySubstring(
        query: String,
        finder: (CPointer<CGeoDbEngine>, CPointer<UInt8>) -> CCityResultList
    ): ArrayList<CityResult> {
        let cQuery = stringToCString(query)
        defer {
            // geodb_string_free(cQuery)
        }

        unsafe {
            let list = finder(this.handle, cQuery)
            let results = convertCityResultList(list)
            geodb_city_result_list_free(list)
            return results
        }
    }

    private func convertCityResultList(list: CCityResultList): ArrayList<CityResult> {
        var results = ArrayList<CityResult>()

        if (list.data.isNull() || list.len == 0) {
            return results
        }

        unsafe {
            for (i in 0..list.len) {
                let c_result = list.data[i]
                results.append(CityResult.fromC(c_result))
            }
        }

        return results
    }
}
