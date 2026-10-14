import Combine
import GeodbGlobe
import SwiftUI

/// The app's two tabs and the one globe they share: a search result can be shown on the globe.
@MainActor
final class AppNavigator: ObservableObject {
    enum Tab: Hashable {
        case search, globe
    }

    /// `-globe` on the command line starts on the globe (screenshots, UI tests).
    @Published var tab: Tab = ProcessInfo.processInfo.arguments.contains("-globe") ? .globe : .search
    let globe = GlobeController()

    init() {
        #if os(tvOS)
        // on the TV the globe turns by itself until someone flies somewhere (play/pause stops it)
        globe.spinning = true
        #endif
    }

    func showOnGlobe(latitude: Double, longitude: Double) {
        tab = .globe
        globe.fly(to: latitude, longitude: longitude, altitudeKm: 300)
    }
}

struct RootView: View {
    @StateObject private var navigator = AppNavigator()

    var body: some View {
        TabView(selection: $navigator.tab) {
            ContentView()
                .tabItem { Label("Search", systemImage: "magnifyingglass") }
                .tag(AppNavigator.Tab.search)
            GlobeScreen(globe: navigator.globe)
                .tabItem { Label("Globe", systemImage: "globe.europe.africa") }
                .tag(AppNavigator.Tab.globe)
        }
        .environmentObject(navigator)
    }
}

/// The 3D globe (geodb-globe on Metal, the same renderer as the web demo) with what it shows in words.
struct GlobeScreen: View {
    @ObservedObject var globe: GlobeController

    var body: some View {
        // the globe under the bars, edge to edge; what is written over it stays in the safe area
        ZStack {
            GlobeView(controller: globe)
                .ignoresSafeArea()
        }
            .overlay(alignment: .bottom) {
                GlobeInfoBar(globe: globe)
                    .padding()
            }
            #if !os(tvOS)
            .overlay(alignment: .topTrailing) {
                GlobeControls(globe: globe)
                    .padding()
            }
            #endif
    }
}

private struct GlobeInfoBar: View {
    @ObservedObject var globe: GlobeController

    var body: some View {
        VStack(spacing: 2) {
            if let city = globe.selected {
                Text(city.name)
                    .font(.headline)
                Text(city.detail)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            } else {
                Text(position)
                    .font(.subheadline.monospacedDigit())
                if globe.status.cities > 0 {
                    Text("\(globe.status.cities) cities within \(km(globe.status.radiusKm))")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            #if os(tvOS)
            Text("Swipe to turn · click to zoom in, hold to zoom out · play/pause \(globe.spinning ? "stops" : "starts") the spin")
                .font(.caption2)
                .foregroundStyle(.secondary)
            #endif
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12))
    }

    private var position: String {
        let s = globe.status
        let lat = String(format: "%.2f° %@", abs(s.latitude), s.latitude >= 0 ? "N" : "S")
        let lon = String(format: "%.2f° %@", abs(s.longitude), s.longitude >= 0 ? "E" : "W")
        return "\(lat)  \(lon)  ·  \(km(s.altitudeKm)) up"
    }

    private func km(_ v: Double) -> String {
        v >= 100 ? String(format: "%.0f km", v) : String(format: "%.1f km", v)
    }
}

#if !os(tvOS)
private struct GlobeControls: View {
    @ObservedObject var globe: GlobeController

    var body: some View {
        VStack(spacing: 8) {
            Button { globe.zoom(by: 0.5) } label: { Image(systemName: "plus") }
            Button { globe.zoom(by: 2) } label: { Image(systemName: "minus") }
            Toggle(isOn: $globe.spinning) { Image(systemName: "arrow.triangle.2.circlepath") }
                .toggleStyle(.button)
            Toggle(isOn: $globe.relief) { Image(systemName: "mountain.2") }
                .toggleStyle(.button)
            Toggle(isOn: $globe.isohypses) { Image(systemName: "point.topleft.down.to.point.bottomright.curvepath") }
                .toggleStyle(.button)
        }
        .buttonStyle(.bordered)
        .labelStyle(.iconOnly)
    }
}
#endif
