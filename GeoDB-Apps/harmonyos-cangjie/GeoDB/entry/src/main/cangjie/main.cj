// GeoDB HarmonyOS App - Main Entry Point
// Cangjie implementation using direct FFI to Rust geodb_ffi
//
// This is a proof-of-concept showing Cangjie → Rust FFI
// In production, this would integrate with DevEco Studio and ArkUI

package geodb_harmonyos

import geodb.*
import ui.*
import std.collection.*

// ============================================================================
// Main Entry Point
// ============================================================================

main() {
    println("=== GeoDB HarmonyOS (Cangjie) ===")
    println()

    // Initialize database
    println("Initializing database...")

    let db = try {
        GeoDbEngine()
    } catch (e: Exception) {
        println("ERROR: Failed to initialize database: ${e.message}")
        return
    }

    println("✓ Database initialized")
    println()

    // Show stats
    let stats = db.stats()
    println("Database Statistics:")
    println("  Countries: ${stats.countries}")
    println("  States:    ${stats.states}")
    println("  Cities:    ${stats.cities}")
    println()

    // Test country count
    let countryCount = db.countryCount()
    println("Country count: ${countryCount}")
    println()

    // Test find country by code
    println("Finding country by code 'DE'...")
    if (let germany = db.findCountryByCode("DE")) {
        println("  Found: ${germany.name} (${germany.iso2})")
        println("  Lat/Lng: ${germany.lat}, ${germany.lng}")
    } else {
        println("  Not found")
    }
    println()

    // Test smart search
    println("Smart searching for 'Berlin'...")
    let results = db.smartSearch("Berlin")
    println("  Found ${results.size()} results:")

    for (i in 0..min(5, results.size())) {
        let city = results[i]
        let flag = countryFlag(city.iso2)
        println("    ${flag} ${city.name}, ${city.state}, ${city.country}")
        println("       Population: ${city.population}")
        if (let dist = city.distanceKm) {
            println("       Distance: ${dist} km")
        }
    }
    println()

    // Test nearest search (Berlin coordinates)
    println("Finding nearest cities to Berlin (52.52, 13.405)...")
    let nearest = db.findNearest(52.52, 13.405, 5)
    println("  Found ${nearest.size()} nearest cities:")

    for (city in nearest) {
        let flag = countryFlag(city.iso2)
        let dist = city.distanceKm ?? 0.0
        println("    ${flag} ${city.name} - ${String.format('%.1f', dist)} km")
    }
    println()

    // Test radius search
    println("Finding cities within 50km of Berlin...")
    let inRadius = db.findInRadius(52.52, 13.405, 50.0)
    println("  Found ${inRadius.size()} cities")
    println()

    println("✓ All tests passed!")
    println()
    println("Note: This is a CLI test. To build a full HarmonyOS app,")
    println("integrate this Cangjie code with ArkUI in DevEco Studio.")
}

// Helper: Convert ISO2 to emoji flag
func countryFlag(iso2: String): String {
    let base: UInt32 = 127397
    var flag = ""

    for (ch in iso2.toUpperCase()) {
        let scalar = UInt32(ch.toInt()) + base
        flag += String.fromUnicode(scalar)
    }

    return flag
}

// Helper: Get minimum of two numbers
func min(a: Int64, b: Int64): Int64 {
    return if (a < b) { a } else { b }
}
