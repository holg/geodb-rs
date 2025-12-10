// Safe Cangjie types wrapping the FFI
package geodb

import std.collection.*

// ============================================================================
// Safe Cangjie Types (mirroring Swift/Kotlin)
// ============================================================================

// City result with safe Cangjie types
public class CityResult {
    public let name: String
    public let state: String
    public let country: String
    public let iso2: String
    public let lat: Float64
    public let lng: Float64
    public let population: UInt64
    public let distanceKm: Float64?  // Optional
    public let translations: HashMap<String, String>

    public init(
        name: String,
        state: String,
        country: String,
        iso2: String,
        lat: Float64,
        lng: Float64,
        population: UInt64,
        distanceKm: Float64?,
        translations: HashMap<String, String>
    ) {
        this.name = name
        this.state = state
        this.country = country
        this.iso2 = iso2
        this.lat = lat
        this.lng = lng
        this.population = population
        this.distanceKm = distanceKm
        this.translations = translations
    }

    // Create from C FFI struct
    internal static func fromC(c: CCityResult): CityResult {
        // Convert C strings to Cangjie strings
        let name = cStringToString(c.name)
        let state = cStringToString(c.state)
        let country = cStringToString(c.country)
        let iso2 = cStringToString(c.iso2)

        // Parse translations JSON (simplified - in real impl use JSON parser)
        let translations = parseTranslationsJson(c.translations_json)

        // Handle optional distance
        let distanceKm = if (c.distance_km < 0.0) { None } else { Some(c.distance_km) }

        return CityResult(
            name: name,
            state: state,
            country: country,
            iso2: iso2,
            lat: c.lat,
            lng: c.lng,
            population: c.population,
            distanceKm: distanceKm,
            translations: translations
        )
    }
}

// Database statistics
public class DbStats {
    public let countries: UInt64
    public let states: UInt64
    public let cities: UInt64

    public init(countries: UInt64, states: UInt64, cities: UInt64) {
        this.countries = countries
        this.states = states
        this.cities = cities
    }

    internal static func fromC(c: CDbStatsDto): DbStats {
        return DbStats(countries: c.countries, states: c.states, cities: c.cities)
    }
}

// ============================================================================
// Helper functions for C <-> Cangjie conversion
// ============================================================================

// Convert C string to Cangjie String
internal func cStringToString(ptr: CPointer<UInt8>): String {
    if (ptr.isNull()) {
        return ""
    }

    // TODO: Implement proper C string conversion
    // This is a placeholder - actual implementation depends on Cangjie std lib
    var chars = ArrayList<UInt8>()
    var i: Int64 = 0

    unsafe {
        while (true) {
            let ch = ptr[i]
            if (ch == 0) {
                break
            }
            chars.append(ch)
            i += 1
        }
    }

    // Convert bytes to String (assumes UTF-8)
    return String.fromUtf8(chars.toArray())
}

// Convert Cangjie String to C string (caller must free)
internal func stringToCString(s: String): CPointer<UInt8> {
    let bytes = s.toUtf8()
    // TODO: Allocate C string and copy bytes
    // This requires malloc/free bindings
    // Placeholder implementation
    return CPointer<UInt8>()
}

// Parse JSON translations (simplified)
internal func parseTranslationsJson(json: CPointer<UInt8>): HashMap<String, String> {
    let jsonStr = cStringToString(json)

    // TODO: Implement proper JSON parsing
    // For now, return empty map
    return HashMap<String, String>()
}
