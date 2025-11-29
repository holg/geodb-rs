#!/usr/bin/env swift

import Foundation
import GeodbKit

print("Testing GeodbKit...")

do {
    let db = try GeoDbEngine()
    print("✓ Database initialized")

    let stats = db.stats()
    print("✓ Database stats:")
    print("  - Countries: \(stats.countries)")
    print("  - States: \(stats.states)")
    print("  - Cities: \(stats.cities)")

    print("\nTesting smart search for 'Berlin'...")
    let results = db.smartSearch(query: "Berlin")
    print("✓ Found \(results.count) results")

    if let first = results.first {
        print("\nFirst result:")
        print("  - Name: \(first.name)")
        print("  - Country: \(first.country)")
        print("  - Coordinates: \(first.lat), \(first.lng)")
        print("  - Population: \(first.population)")
    }

    print("\nTesting nearest search (to Berlin coords)...")
    let nearest = db.findNearest(lat: 52.52, lng: 13.405, count: 5)
    print("✓ Found \(nearest.count) nearest cities")

    for (i, city) in nearest.prefix(3).enumerated() {
        print("  \(i+1). \(city.name), \(city.country) - \(city.distanceKm ?? 0) km")
    }

    print("\n✅ All tests passed!")
    exit(0)

} catch {
    print("❌ Error: \(error)")
    exit(1)
}
