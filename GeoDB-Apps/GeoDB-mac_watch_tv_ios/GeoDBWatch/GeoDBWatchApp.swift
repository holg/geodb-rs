import SwiftUI

@main
struct GeoDBWatchApp: App {
    @StateObject private var database = GeoDatabase()

    var body: some Scene {
        WindowGroup {
            WatchContentView()
                .environmentObject(database)
        }
    }
}
