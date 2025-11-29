import SwiftUI

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
