import XCTest
@testable import GeoDB
import GeodbKit

final class GeoDBDatabaseTests: XCTestCase {

    func testDatabaseInitialization() throws {
        // Test that database initializes
        let db = try GeoDbEngine()
        XCTAssertNotNil(db, "Database should initialize")
    }

    func testDatabaseStats() throws {
        let db = try GeoDbEngine()
        let stats = db.stats()

        XCTAssertGreaterThan(stats.countries, 0, "Should have countries")
        XCTAssertGreaterThan(stats.states, 0, "Should have states")
        XCTAssertGreaterThan(stats.cities, 0, "Should have cities")

        print("Database stats:")
        print("  Countries: \(stats.countries)")
        print("  States: \(stats.states)")
        print("  Cities: \(stats.cities)")
    }

    func testSmartSearch() throws {
        let db = try GeoDbEngine()
        let results = db.smartSearch(query: "Berlin")

        XCTAssertGreaterThan(results.count, 0, "Should find cities matching 'Berlin'")

        if let first = results.first {
            print("First result: \(first.name), \(first.country)")
            XCTAssertEqual(first.name, "Berlin", "First result should be Berlin")
        }
    }

    func testFindNearest() throws {
        let db = try GeoDbEngine()
        // Berlin coordinates
        let results = db.findNearest(lat: 52.52, lng: 13.405, count: 10)

        XCTAssertEqual(results.count, 10, "Should return 10 nearest cities")

        print("Nearest cities to Berlin:")
        for (i, city) in results.prefix(5).enumerated() {
            print("  \(i+1). \(city.name), \(city.country) - \(city.distanceKm ?? 0) km")
        }
    }

    func testFindInRadius() throws {
        let db = try GeoDbEngine()
        // Berlin coordinates, 50km radius
        let results = db.findInRadius(lat: 52.52, lng: 13.405, radiusKm: 50.0)

        XCTAssertGreaterThan(results.count, 0, "Should find cities within 50km of Berlin")

        print("Cities within 50km of Berlin: \(results.count)")
    }
}
