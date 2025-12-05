import XCTest
@testable import GeodbKit

final class GeodbKitTests: XCTestCase {
    func testInitialization() throws {
        let db = try GeoDbEngine()

        let count = db.countryCount()
        print("Loaded \(count) countries from Rust!")
        XCTAssertGreaterThan(count, 0, "Database should not be empty")
    }

    func testSearch() throws {
        let db = try GeoDbEngine()

        let results = db.smartSearch(query: "Berlin")
        XCTAssertFalse(results.isEmpty, "Should find Berlin")

        let first = results[0]
        XCTAssertEqual(first.name, "Berlin")
        XCTAssertEqual(first.country, "Germany")
    }
}
