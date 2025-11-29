import SwiftUI
import Combine
import GeodbKit

@main
struct GeoDBApp: App {
    @StateObject private var geoDatabase = GeoDatabase()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(geoDatabase)
        }
        #if os(macOS)
        .commands {
            CommandGroup(replacing: .newItem) {}
        }
        #endif
    }
}

/// Main database manager
@MainActor
class GeoDatabase: ObservableObject {
    @Published var isInitialized = false
    @Published var stats: DbStatsDto?
    @Published var error: String?

    private var db: GeoDbEngine?

    init() {
        initialize()
    }

    func initialize() {
        do {
            db = try GeoDbEngine()
            stats = db?.stats()
            isInitialized = true
        } catch {
            self.error = "Failed to initialize database: \(error.localizedDescription)"
        }
    }

    func smartSearch(_ query: String) -> [CityResult] {
        guard let db = db else { return [] }
        return db.smartSearch(query: query)
    }

    func findNearest(lat: Double, lng: Double, count: UInt32) -> [CityResult] {
        guard let db = db else { return [] }
        return db.findNearest(lat: lat, lng: lng, count: count)
    }

    func findInRadius(lat: Double, lng: Double, radiusKm: Double) -> [CityResult] {
        guard let db = db else { return [] }
        return db.findInRadius(lat: lat, lng: lng, radiusKm: radiusKm)
    }

    func findCountries(_ query: String) -> [CityResult] {
        guard let db = db else { return [] }
        return db.findCountriesBySubstring(substr: query)
    }

    func findStates(_ query: String) -> [CityResult] {
        guard let db = db else { return [] }
        return db.findStatesBySubstring(substr: query)
    }

    func findCities(_ query: String) -> [CityResult] {
        guard let db = db else { return [] }
        return db.findCitiesBySubstring(substr: query)
    }
}
