//
//  GeoDbApp.swift
//  GeoDb Watch App
//
//  Created by Holger Trahe on 28.11.25.
//

import SwiftUI

@main
struct GeoDb_Watch_AppApp: App {
    @StateObject private var database = GeoDatabase()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(database)
        }
    }
}
