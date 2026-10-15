//
//  GeoDbApp.swift
//  GeoDb Watch App
//
//  Created by Holger Trahe on 28.11.25.
//

import GeodbWatchGlobe
import SwiftUI

@main
struct GeoDb_Watch_AppApp: App {
    @StateObject private var database = GeoDatabase()
    /// One globe for the app: its earth is built once.
    @StateObject private var globe = WatchGlobeController()

    var body: some Scene {
        WindowGroup {
            Group {
                // `-globe` on the command line starts on the globe (screenshots, UI tests)
                if ProcessInfo.processInfo.arguments.contains("-globe") {
                    NavigationView { WatchGlobeScreen() }
                } else {
                    ContentView()
                }
            }
            .environmentObject(database)
            .environmentObject(globe)
        }
    }
}
