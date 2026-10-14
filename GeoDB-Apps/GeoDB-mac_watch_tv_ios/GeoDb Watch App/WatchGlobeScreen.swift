import GeodbKit
import GeodbWatchGlobe
import SwiftUI

/// The board's globe (geodb-fw-core, the software renderer of the STM32 boards) on the watch, turning by
/// itself; opened from a city it looks at that city.
struct WatchGlobeScreen: View {
    @EnvironmentObject var globe: WatchGlobeController
    var focus: CityResult?

    var body: some View {
        WatchGlobeView(controller: globe)
            .ignoresSafeArea(edges: .bottom)
            .navigationTitle(focus?.name ?? "Globe")
            .onAppear {
                if let city = focus {
                    globe.fly(to: city.lat, longitude: city.lng)
                }
            }
    }
}
