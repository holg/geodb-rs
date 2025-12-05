import SwiftUI
import GeodbKit
import MapKit

#if os(tvOS)
// tvOS-specific SearchMode enum
enum SearchMode: String, CaseIterable {
    case smart = "Smart"
    case cities = "Cities"
    case states = "States"
    case countries = "Countries"
    case nearest = "Nearest"
    case radius = "Radius"

    var icon: String {
        switch self {
        case .smart: return "sparkle.magnifyingglass"
        case .cities: return "building.2.fill"
        case .states: return "map.fill"
        case .countries: return "flag.fill"
        case .nearest: return "location.circle.fill"
        case .radius: return "circle.dashed"
        }
    }
}

struct ContentView: View {
    @EnvironmentObject var geoDatabase: GeoDatabase
    @State private var searchText = ""
    @State private var searchResults: [CityResult] = []
    @State private var selectedCity: CityResult?
    @State private var showingDetail = false
    @State private var searchMode: SearchMode = .smart
    @State private var spatialLat = "52.52"
    @State private var spatialLng = "13.405"
    @State private var nearestCount = "10"
    @State private var radiusKm = "50"
    @State private var hasSearched = false
    @FocusState private var isResultsFocused: Bool

    var body: some View {
        NavigationStack {
            VStack(spacing: 20) {
                // Search Mode Switches - ALWAYS at the very top
                searchModeSwitchesView

                // Search Field or Spatial Parameters
                if searchMode == .nearest || searchMode == .radius {
                    spatialParametersView
                } else {
                    searchFieldView
                }

                // Content area
                if !geoDatabase.isInitialized {
                    Spacer()
                    loadingView
                    Spacer()
                } else if let error = geoDatabase.error {
                    Spacer()
                    errorView(error)
                    Spacer()
                } else if !hasSearched && searchResults.isEmpty {
                    // Startup screen - show header and stats
                    Spacer()
                    startupView
                    Spacer()
                } else if searchResults.isEmpty && hasSearched {
                    Spacer()
                    noResultsView
                    Spacer()
                } else {
                    resultsGridView
                    Spacer()
                }
            }
            .padding(.horizontal, 80)
            .padding(.top, 50)
            .padding(.bottom, 50)
            .background(Color.black.opacity(0.8))
            .navigationDestination(isPresented: $showingDetail) {
                if let city = selectedCity {
                    CityDetailView(city: city)
                        .environmentObject(geoDatabase)
                }
            }
        }
    }

    // MARK: - Startup View (header + stats)
    private var startupView: some View {
        VStack(spacing: 30) {
            // Header
            HStack(spacing: 20) {
                Image(systemName: "globe.americas.fill")
                    .font(.system(size: 60))
                    .foregroundColor(.blue)

                VStack(alignment: .leading, spacing: 6) {
                    Text("GeoDB")
                        .font(.system(size: 44, weight: .bold))
                    Text("Geographic Database")
                        .font(.system(size: 22))
                        .foregroundColor(.secondary)
                }
            }

            // Database Statistics
            if let stats = geoDatabase.stats {
                VStack(spacing: 15) {
                    Text("Database Statistics")
                        .font(.title3)
                        .foregroundColor(.secondary)

                    HStack(spacing: 40) {
                        StatCard(title: "Countries", value: "\(stats.countries)", icon: "flag.fill", color: .blue)
                        StatCard(title: "States", value: "\(stats.states)", icon: "map.fill", color: .green)
                        StatCard(title: "Cities", value: "\(stats.cities)", icon: "building.2.fill", color: .orange)
                    }
                }
            }

            Text("Select a search mode and start searching")
                .font(.body)
                .foregroundColor(.secondary)
        }
    }

    // MARK: - Search Mode Switches
    private var searchModeSwitchesView: some View {
        HStack(spacing: 15) {
            ForEach(SearchMode.allCases, id: \.self) { mode in
                Button {
                    searchMode = mode
                    searchResults = []
                    hasSearched = false
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: mode.icon)
                            .font(.callout)
                        Text(mode.rawValue)
                            .font(.callout)
                    }
                    .padding(.horizontal, 18)
                    .padding(.vertical, 12)
                    .background(searchMode == mode ? Color.blue : Color.white.opacity(0.1))
                    .cornerRadius(10)
                }
                .buttonStyle(.card)
            }
        }
        .focusSection()
        .disabled(showingDetail)
    }

    // MARK: - Spatial Parameters View
    private var spatialParametersView: some View {
        HStack(spacing: 30) {
            VStack(alignment: .leading, spacing: 8) {
                Text("Latitude")
                    .font(.headline)
                    .foregroundColor(.secondary)
                TextField("52.52", text: $spatialLat)
                    .font(.title3)
                    .padding(.horizontal, 20)
                    .padding(.vertical, 15)
                    .background(Color.white.opacity(0.1))
                    .cornerRadius(10)
                    .frame(width: 200)
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Longitude")
                    .font(.headline)
                    .foregroundColor(.secondary)
                TextField("13.405", text: $spatialLng)
                    .font(.title3)
                    .padding(.horizontal, 20)
                    .padding(.vertical, 15)
                    .background(Color.white.opacity(0.1))
                    .cornerRadius(10)
                    .frame(width: 200)
            }

            if searchMode == .nearest {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Count")
                        .font(.headline)
                        .foregroundColor(.secondary)
                    TextField("10", text: $nearestCount)
                        .font(.title3)
                        .padding(.horizontal, 20)
                        .padding(.vertical, 15)
                        .background(Color.white.opacity(0.1))
                        .cornerRadius(10)
                        .frame(width: 120)
                }
            }

            if searchMode == .radius {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Radius (km)")
                        .font(.headline)
                        .foregroundColor(.secondary)
                    TextField("50", text: $radiusKm)
                        .font(.title3)
                        .padding(.horizontal, 20)
                        .padding(.vertical, 15)
                        .background(Color.white.opacity(0.1))
                        .cornerRadius(10)
                        .frame(width: 150)
                }
            }

            // Search button
            Button {
                performSpatialSearch()
            } label: {
                HStack {
                    Image(systemName: "magnifyingglass")
                    Text("Search")
                }
                .font(.title3)
                .padding(.horizontal, 30)
                .padding(.vertical, 15)
                .background(Color.blue)
                .cornerRadius(10)
            }
            .buttonStyle(.card)
        }
        .focusSection()
        .disabled(showingDetail)
    }

    // MARK: - Search Field
    private var searchFieldView: some View {
        HStack(spacing: 15) {
            Image(systemName: "magnifyingglass")
                .font(.title2)
                .foregroundColor(.secondary)

            TextField("Search \(searchMode.rawValue.lowercased())...", text: $searchText)
                .font(.title2)
                .onChange(of: searchText) { newValue in
                    performSearch(newValue)
                }
        }
        .padding(.horizontal, 30)
        .padding(.vertical, 20)
        .background(Color.white.opacity(0.1))
        .cornerRadius(15)
        .frame(maxWidth: .infinity)
        .disabled(showingDetail)
    }

    // MARK: - Loading View
    private var loadingView: some View {
        VStack(spacing: 20) {
            ProgressView()
                .scaleEffect(2)
            Text("Loading database...")
                .font(.title2)
        }
    }

    // MARK: - Error View
    private func errorView(_ error: String) -> some View {
        VStack(spacing: 20) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 60))
                .foregroundColor(.red)
            Text(error)
                .font(.title3)
                .multilineTextAlignment(.center)
        }
    }

    // MARK: - No Results View
    private var noResultsView: some View {
        VStack(spacing: 20) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 60))
                .foregroundColor(.secondary)
            Text("No results found")
                .font(.title2)
        }
    }

    // MARK: - Results Grid
    private var resultsGridView: some View {
        VStack(alignment: .leading, spacing: 15) {
            Text("Results (\(searchResults.count))")
                .font(.title3)
                .foregroundColor(.secondary)

            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(spacing: 40) {
                    ForEach(searchResults, id: \.id) { city in
                        Button {
                            selectedCity = city
                            showingDetail = true
                        } label: {
                            CityCardView(city: city, isSelected: false)
                        }
                        .buttonStyle(.card)
                    }
                }
                .padding(.horizontal, 30)
            }
            .frame(maxWidth: .infinity)
            .frame(height: 680)
            .focusSection()
        }
        .frame(maxWidth: .infinity)
        .opacity(showingDetail ? 0.3 : 1.0)
        .allowsHitTesting(!showingDetail)
    }

    // MARK: - Search Functions
    private func performSearch(_ query: String) {
        guard !query.isEmpty else {
            searchResults = []
            hasSearched = false
            return
        }
        hasSearched = true

        switch searchMode {
        case .smart:
            searchResults = geoDatabase.smartSearch(query)
        case .cities:
            searchResults = geoDatabase.findCities(query)
        case .states:
            searchResults = geoDatabase.findStates(query)
        case .countries:
            searchResults = geoDatabase.findCountries(query)
        case .nearest, .radius:
            break // Handled by performSpatialSearch
        }
    }

    private func performSpatialSearch() {
        hasSearched = true

        guard let lat = Double(spatialLat),
              let lng = Double(spatialLng) else {
            searchResults = []
            return
        }

        switch searchMode {
        case .nearest:
            let count = UInt32(nearestCount) ?? 10
            searchResults = geoDatabase.findNearest(lat: lat, lng: lng, count: count)
        case .radius:
            let radius = Double(radiusKm) ?? 50
            searchResults = geoDatabase.findInRadius(lat: lat, lng: lng, radiusKm: radius)
        default:
            break
        }
    }
}

// MARK: - Card Button Style for tvOS
struct CardButtonStyle: ButtonStyle {
    @Environment(\.isFocused) var isFocused

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.95 : 1.0)
            .animation(.easeInOut(duration: 0.1), value: configuration.isPressed)
    }
}

// MARK: - City Detail View
struct CityDetailView: View {
    @EnvironmentObject var geoDatabase: GeoDatabase
    @State var city: CityResult

    @State private var nearestCities: [CityResult] = []
    @State private var radiusCities: [CityResult] = []
    @State private var selectedRadius: Double = 50.0
    @State private var selectedTab = 0
    @State private var showingOverlay = false
    @State private var overlayCity: CityResult?
    @FocusState private var detailViewFocused: Bool

    let radiusOptions: [Double] = [25, 50, 100, 200, 500]

    var body: some View {
        ZStack {
            ScrollView {
                VStack(spacing: 40) {
                    // City Header
                    cityHeaderView

                    // Tab Selection
                    tabSelectionView

                    // Content based on tab
                    if selectedTab == 0 {
                        nearestCitiesView
                    } else {
                        radiusSearchView
                    }
                }
                .padding(60)
            }
            .background(Color.black.opacity(0.8))
            .opacity(showingOverlay ? 0.3 : 1.0)
            .allowsHitTesting(!showingOverlay)

            // Overlay
            if showingOverlay, let cityToShow = overlayCity {
                CityOverlayView(
                    city: cityToShow,
                    referenceCity: city,
                    onNavigate: { [cityToShow] in
                        showingOverlay = false
                        overlayCity = nil
                        selectCity(cityToShow)
                    },
                    onDismiss: {
                        showingOverlay = false
                        overlayCity = nil
                    }
                )
                .transition(.opacity.combined(with: .scale(scale: 0.9)))
            }
        }
        .animation(.easeInOut(duration: 0.25), value: showingOverlay)
        .onAppear {
            loadNearestCities()
            loadRadiusCities()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                detailViewFocused = true
            }
        }
    }

    private var cityHeaderView: some View {
        VStack(spacing: 15) {
            HStack(spacing: 20) {
                Text(flagEmoji(for: city.iso2))
                    .font(.system(size: 55))

                VStack(alignment: .leading, spacing: 8) {
                    Text(city.name)
                        .font(.system(size: 34, weight: .bold))
                        .lineLimit(2)
                        .minimumScaleFactor(0.6)
                        .fixedSize(horizontal: false, vertical: true)

                    if !city.state.isEmpty {
                        Text(city.state)
                            .font(.title3)
                            .foregroundColor(.secondary)
                            .lineLimit(2)
                    }

                    Text(city.country)
                        .font(.title3)
                        .foregroundColor(.secondary)
                        .lineLimit(2)
                }

                Spacer()

                VStack(alignment: .trailing, spacing: 10) {
                    if city.population > 0 {
                        HStack {
                            Image(systemName: "person.3.fill")
                            Text(formatPopulation(city.population))
                        }
                        .font(.body)
                        .foregroundColor(.blue)
                    }

                    HStack {
                        Image(systemName: "location.fill")
                        Text(String(format: "%.4f, %.4f", city.lat, city.lng))
                    }
                    .font(.body)
                    .foregroundColor(.secondary)
                }
            }
            .padding(25)
            .background(Color.white.opacity(0.1))
            .cornerRadius(15)
        }
    }

    private var tabSelectionView: some View {
        HStack(spacing: 20) {
            Button {
                selectedTab = 0
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "location.circle.fill")
                    Text("Nearest Cities")
                }
                .font(.body)
                .padding(.horizontal, 24)
                .padding(.vertical, 14)
                .background(selectedTab == 0 ? Color.blue : Color.white.opacity(0.1))
                .cornerRadius(10)
            }
            .buttonStyle(.card)

            Button {
                selectedTab = 1
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "circle.dashed")
                    Text("Radius Search")
                }
                .font(.body)
                .padding(.horizontal, 24)
                .padding(.vertical, 14)
                .background(selectedTab == 1 ? Color.blue : Color.white.opacity(0.1))
                .cornerRadius(10)
            }
            .buttonStyle(.card)
        }
        .focusSection()
        .defaultFocus($detailViewFocused, true)
    }

    private var nearestCitiesView: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("10 Nearest Cities")
                .font(.title3)
                .foregroundColor(.secondary)

            if nearestCities.isEmpty {
                Text("Loading...")
                    .font(.body)
                    .foregroundColor(.secondary)
            } else {
                ScrollView(.horizontal, showsIndicators: false) {
                    LazyHStack(spacing: 30) {
                        ForEach(nearestCities, id: \.id) { nearCity in
                            Button {
                                overlayCity = nearCity
                                showingOverlay = true
                            } label: {
                                NearCityCard(city: nearCity, referenceCity: city)
                            }
                            .buttonStyle(.card)
                        }
                    }
                    .padding(.horizontal, 20)
                }
                .frame(maxWidth: .infinity)
                .frame(height: 390)
                .focusSection()
            }
        }
        .frame(maxWidth: .infinity)
    }

    private var radiusSearchView: some View {
        VStack(alignment: .leading, spacing: 20) {
            // Radius Selection
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 15) {
                    Text("Radius:")
                        .font(.title3)
                        .foregroundColor(.secondary)

                    ForEach(radiusOptions, id: \.self) { radius in
                        Button {
                            withAnimation {
                                selectedRadius = radius
                            }
                            DispatchQueue.main.async {
                                loadRadiusCities()
                            }
                        } label: {
                            Text("\(Int(radius)) km")
                                .font(.title3)
                                .padding(.horizontal, 24)
                                .padding(.vertical, 14)
                                .background(selectedRadius == radius ? Color.blue : Color.white.opacity(0.1))
                                .cornerRadius(8)
                        }
                        .buttonStyle(.card)
                    }
                }
            }
            .focusSection()

            VStack(alignment: .leading, spacing: 15) {
                Text("\(radiusCities.count) cities within \(Int(selectedRadius)) km")
                    .font(.title3)
                    .foregroundColor(.secondary)

                if radiusCities.isEmpty {
                    Text("Loading cities...")
                        .font(.body)
                        .foregroundColor(.secondary)
                        .padding(.top, 20)
                } else {
                    ScrollView(.horizontal, showsIndicators: false) {
                        LazyHStack(spacing: 30) {
                            ForEach(radiusCities, id: \.id) { nearCity in
                                Button {
                                    overlayCity = nearCity
                                    showingOverlay = true
                                } label: {
                                    NearCityCard(city: nearCity, referenceCity: city)
                                }
                                .buttonStyle(.card)
                            }
                        }
                        .padding(.horizontal, 20)
                    }
                    .frame(maxWidth: .infinity)
                    .frame(height: 390)
                    .focusSection()
                }
            }
        }
        .frame(maxWidth: .infinity)
    }

    private func selectCity(_ newCity: CityResult) {
        city = newCity
        loadNearestCities()
        loadRadiusCities()
    }

    private func loadNearestCities() {
        nearestCities = geoDatabase.findNearest(lat: city.lat, lng: city.lng, count: 11)
            .filter { $0.id != city.id }
            .prefix(10)
            .map { $0 }
    }

    private func loadRadiusCities() {
        radiusCities = geoDatabase.findInRadius(lat: city.lat, lng: city.lng, radiusKm: selectedRadius)
            .filter { $0.id != city.id }
    }

    private func flagEmoji(for countryCode: String) -> String {
        let base: UInt32 = 127397
        var emoji = ""
        for scalar in countryCode.uppercased().unicodeScalars {
            if let unicode = UnicodeScalar(base + scalar.value) {
                emoji.append(String(unicode))
            }
        }
        return emoji.isEmpty ? "🌍" : emoji
    }

    private func formatPopulation(_ pop: UInt64) -> String {
        if pop >= 1_000_000 {
            return String(format: "%.1fM", Double(pop) / 1_000_000)
        } else if pop >= 1_000 {
            return String(format: "%.0fK", Double(pop) / 1_000)
        }
        return "\(pop)"
    }
}

// MARK: - Near City Card
struct NearCityCard: View {
    let city: CityResult
    let referenceCity: CityResult
    @Environment(\.isFocused) var isFocused

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text(flagEmoji(for: city.iso2))
                    .font(.system(size: 44))
                Spacer()
                Text(distanceString)
                    .font(.title3)
                    .foregroundColor(.blue)
            }

            Spacer()

            Text(city.name)
                .font(.title2)
                .fontWeight(.bold)
                .lineLimit(2)
                .minimumScaleFactor(0.7)
                .fixedSize(horizontal: false, vertical: true)

            Text(city.country)
                .font(.title3)
                .foregroundColor(.secondary)
                .lineLimit(2)
                .minimumScaleFactor(0.8)
        }
        .padding(32)
        .frame(width: 360, height: 330)
        .background(isFocused ? Color.blue.opacity(0.3) : Color.white.opacity(0.1))
        .cornerRadius(14)
        .overlay(
            RoundedRectangle(cornerRadius: 14)
                .stroke(isFocused ? Color.blue : Color.clear, lineWidth: 3)
        )
        .scaleEffect(isFocused ? 1.05 : 1.0)
        .animation(.easeInOut(duration: 0.15), value: isFocused)
    }

    private var distanceString: String {
        let distance = haversineDistance(
            lat1: referenceCity.lat, lng1: referenceCity.lng,
            lat2: city.lat, lng2: city.lng
        )
        if distance < 1 {
            return String(format: "%.0f m", distance * 1000)
        } else if distance < 10 {
            return String(format: "%.1f km", distance)
        } else {
            return String(format: "%.0f km", distance)
        }
    }

    private func haversineDistance(lat1: Double, lng1: Double, lat2: Double, lng2: Double) -> Double {
        let R = 6371.0 // Earth's radius in km
        let dLat = (lat2 - lat1) * .pi / 180
        let dLng = (lng2 - lng1) * .pi / 180
        let a = sin(dLat/2) * sin(dLat/2) +
                cos(lat1 * .pi / 180) * cos(lat2 * .pi / 180) *
                sin(dLng/2) * sin(dLng/2)
        let c = 2 * atan2(sqrt(a), sqrt(1-a))
        return R * c
    }

    private func flagEmoji(for countryCode: String) -> String {
        let base: UInt32 = 127397
        var emoji = ""
        for scalar in countryCode.uppercased().unicodeScalars {
            if let unicode = UnicodeScalar(base + scalar.value) {
                emoji.append(String(unicode))
            }
        }
        return emoji.isEmpty ? "🌍" : emoji
    }
}

// MARK: - Stat Card
struct StatCard: View {
    let title: String
    let value: String
    let icon: String
    let color: Color

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: icon)
                .font(.system(size: 36))
                .foregroundColor(color)

            Text(value)
                .font(.system(size: 30, weight: .bold))

            Text(title)
                .font(.body)
                .foregroundColor(.secondary)
        }
        .frame(width: 180, height: 160)
        .background(Color.white.opacity(0.1))
        .cornerRadius(15)
    }
}

// MARK: - City Card View
struct CityCardView: View {
    let city: CityResult
    let isSelected: Bool
    @Environment(\.isFocused) var isFocused

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text(flagEmoji(for: city.iso2))
                    .font(.system(size: 52))

                Spacer()

                if city.population > 0 {
                    Text(formatPopulation(city.population))
                        .font(.body)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 8)
                        .background(Color.blue.opacity(0.3))
                        .cornerRadius(8)
                }
            }

            Text(city.name)
                .font(.title2)
                .fontWeight(.semibold)
                .lineLimit(2)
                .minimumScaleFactor(0.7)
                .fixedSize(horizontal: false, vertical: true)

            if !city.state.isEmpty {
                Text(city.state)
                    .font(.headline)
                    .foregroundColor(.secondary)
                    .lineLimit(2)
                    .minimumScaleFactor(0.7)
            }

            Text(city.country)
                .font(.headline)
                .foregroundColor(.secondary)
                .lineLimit(2)
                .minimumScaleFactor(0.7)

            Spacer()

            HStack {
                Image(systemName: "location.fill")
                    .font(.body)
                    .foregroundColor(.blue)
                Text(String(format: "%.4f, %.4f", city.lat, city.lng))
                    .font(.body)
                    .foregroundColor(.secondary)
            }
        }
        .padding(38)
        .frame(width: 460, height: 600)
        .background(isFocused ? Color.blue.opacity(0.3) : Color.white.opacity(0.1))
        .cornerRadius(20)
        .overlay(
            RoundedRectangle(cornerRadius: 20)
                .stroke(isFocused ? Color.blue : Color.clear, lineWidth: 4)
        )
        .scaleEffect(isFocused ? 1.05 : 1.0)
        .animation(.easeInOut(duration: 0.15), value: isFocused)
    }

    private func flagEmoji(for countryCode: String) -> String {
        let base: UInt32 = 127397
        var emoji = ""
        for scalar in countryCode.uppercased().unicodeScalars {
            if let unicode = UnicodeScalar(base + scalar.value) {
                emoji.append(String(unicode))
            }
        }
        return emoji.isEmpty ? "🌍" : emoji
    }

    private func formatPopulation(_ pop: UInt64) -> String {
        if pop >= 1_000_000 {
            return String(format: "%.1fM", Double(pop) / 1_000_000)
        } else if pop >= 1_000 {
            return String(format: "%.0fK", Double(pop) / 1_000)
        }
        return "\(pop)"
    }
}

// MARK: - City Overlay View
struct CityOverlayView: View {
    let city: CityResult
    let referenceCity: CityResult
    let onNavigate: () -> Void
    let onDismiss: () -> Void

    @State private var showMap = false
    @State private var showFullScreenMap = false
    @State private var mapRegion: MKCoordinateRegion
    @FocusState private var overlayFocused: Bool

    init(city: CityResult, referenceCity: CityResult, onNavigate: @escaping () -> Void, onDismiss: @escaping () -> Void) {
        self.city = city
        self.referenceCity = referenceCity
        self.onNavigate = onNavigate
        self.onDismiss = onDismiss
        self._mapRegion = State(initialValue: MKCoordinateRegion(
            center: CLLocationCoordinate2D(latitude: city.lat, longitude: city.lng),
            span: MKCoordinateSpan(latitudeDelta: 0.5, longitudeDelta: 0.5)
        ))
    }

    var body: some View {
        ZStack {
            // Dimmed background
            Color.black.opacity(0.85)
                .ignoresSafeArea()

            // Fullscreen map overlay
            if showFullScreenMap {
                fullScreenMapView
            } else {
                // Content card
                VStack(spacing: 0) {
                    // Header with flag and name
                    HStack(spacing: 20) {
                        Text(flagEmoji(for: city.iso2))
                            .font(.system(size: 60))

                        VStack(alignment: .leading, spacing: 8) {
                            Text(city.name)
                                .font(.system(size: 36, weight: .bold))
                                .lineLimit(2)
                                .minimumScaleFactor(0.6)
                                .fixedSize(horizontal: false, vertical: true)

                            if !city.state.isEmpty {
                                Text(city.state)
                                    .font(.title3)
                                    .foregroundColor(.secondary)
                                    .lineLimit(2)
                            }

                            Text(city.country)
                                .font(.title3)
                                .foregroundColor(.secondary)
                                .lineLimit(2)
                        }

                        Spacer()
                    }
                    .padding(30)
                    .background(Color.white.opacity(0.1))

                    // Content area - toggle between details and map
                    if showMap {
                        // Map View - larger size
                        Button {
                            showFullScreenMap = true
                        } label: {
                            Map(coordinateRegion: $mapRegion, annotationItems: [city]) { place in
                                MapAnnotation(coordinate: CLLocationCoordinate2D(latitude: place.lat, longitude: place.lng)) {
                                    VStack {
                                        Image(systemName: "mappin.circle.fill")
                                            .font(.system(size: 40))
                                            .foregroundColor(.red)
                                        Text(place.name)
                                            .font(.callout)
                                            .fontWeight(.semibold)
                                            .padding(6)
                                            .background(Color.black.opacity(0.7))
                                            .cornerRadius(6)
                                    }
                                }
                            }
                            .frame(height: 400)
                            .cornerRadius(16)
                            .overlay(
                                VStack {
                                    Spacer()
                                    HStack {
                                        Spacer()
                                        Text("Click for fullscreen")
                                            .font(.callout)
                                            .padding(10)
                                            .background(Color.black.opacity(0.7))
                                            .cornerRadius(8)
                                            .padding(15)
                                    }
                                }
                            )
                        }
                        .buttonStyle(.card)
                        .padding(30)
                    } else {
                    // Details grid
                    VStack(spacing: 20) {
                        HStack(spacing: 40) {
                            // Population
                            DetailItem(
                                icon: "person.3.fill",
                                title: "Population",
                                value: city.population > 0 ? formatPopulation(city.population) : "Unknown",
                                color: .blue
                            )

                            // Distance
                            DetailItem(
                                icon: "location.fill",
                                title: "Distance",
                                value: distanceString,
                                color: .green
                            )

                            // Coordinates
                            DetailItem(
                                icon: "map.fill",
                                title: "Coordinates",
                                value: String(format: "%.4f, %.4f", city.lat, city.lng),
                                color: .orange
                            )
                        }

                        // Additional info row
                        HStack(spacing: 40) {
                            DetailItem(
                                icon: "flag.fill",
                                title: "Country Code",
                                value: city.iso2.uppercased(),
                                color: .purple
                            )

                            DetailItem(
                                icon: "globe.americas.fill",
                                title: "Latitude",
                                value: String(format: "%.6f", city.lat),
                                color: .cyan
                            )

                            DetailItem(
                                icon: "globe.europe.africa.fill",
                                title: "Longitude",
                                value: String(format: "%.6f", city.lng),
                                color: .mint
                            )
                        }
                    }
                    .padding(30)
                }

                Divider()
                    .background(Color.white.opacity(0.3))

                // Action buttons
                HStack(spacing: 25) {
                    Button(action: onNavigate) {
                        HStack(spacing: 10) {
                            Image(systemName: "arrow.right.circle.fill")
                            Text("Navigate Here")
                        }
                        .font(.body)
                        .padding(.horizontal, 28)
                        .padding(.vertical, 16)
                        .background(Color.blue)
                        .cornerRadius(12)
                    }
                    .buttonStyle(.card)

                    Button {
                        showMap.toggle()
                    } label: {
                        HStack(spacing: 10) {
                            Image(systemName: showMap ? "list.bullet" : "map.fill")
                            Text(showMap ? "Details" : "Map")
                        }
                        .font(.body)
                        .padding(.horizontal, 28)
                        .padding(.vertical, 16)
                        .background(Color.green)
                        .cornerRadius(12)
                    }
                    .buttonStyle(.card)

                    Button(action: onDismiss) {
                        HStack(spacing: 10) {
                            Image(systemName: "xmark.circle.fill")
                            Text("Close")
                        }
                        .font(.body)
                        .padding(.horizontal, 28)
                        .padding(.vertical, 16)
                        .background(Color.white.opacity(0.2))
                        .cornerRadius(12)
                    }
                    .buttonStyle(.card)
                }
                .padding(30)
                .focusSection()
                .defaultFocus($overlayFocused, true)
            }
            .frame(maxWidth: 1200)
            .background(Color(white: 0.15))
            .cornerRadius(25)
            .overlay(
                RoundedRectangle(cornerRadius: 25)
                    .stroke(Color.white.opacity(0.2), lineWidth: 1)
            )
            .shadow(color: .black.opacity(0.5), radius: 40, x: 0, y: 15)
            }
        }
        .onAppear {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                overlayFocused = true
            }
        }
        .onExitCommand {
            if showFullScreenMap {
                showFullScreenMap = false
            } else {
                onDismiss()
            }
        }
    }

    // MARK: - Full Screen Map View
    private var fullScreenMapView: some View {
        ZStack {
            // Full screen map
            Map(coordinateRegion: $mapRegion, annotationItems: [city]) { place in
                MapAnnotation(coordinate: CLLocationCoordinate2D(latitude: place.lat, longitude: place.lng)) {
                    VStack {
                        Image(systemName: "mappin.circle.fill")
                            .font(.system(size: 60))
                            .foregroundColor(.red)
                        Text(place.name)
                            .font(.title2)
                            .fontWeight(.bold)
                            .padding(10)
                            .background(Color.black.opacity(0.8))
                            .cornerRadius(10)
                    }
                }
            }
            .ignoresSafeArea()

            // City info overlay at top
            VStack {
                HStack(spacing: 20) {
                    Text(flagEmoji(for: city.iso2))
                        .font(.system(size: 55))

                    VStack(alignment: .leading, spacing: 6) {
                        Text(city.name)
                            .font(.system(size: 34, weight: .bold))
                            .lineLimit(2)
                            .minimumScaleFactor(0.7)
                        Text("\(city.state.isEmpty ? "" : "\(city.state), ")\(city.country)")
                            .font(.title3)
                            .foregroundColor(.secondary)
                            .lineLimit(2)
                    }

                    Spacer()

                    VStack(alignment: .trailing, spacing: 6) {
                        Text(String(format: "%.4f, %.4f", city.lat, city.lng))
                            .font(.body)
                        if city.population > 0 {
                            Text("Pop: \(formatPopulation(city.population))")
                                .font(.body)
                                .foregroundColor(.blue)
                        }
                    }
                }
                .padding(30)
                .background(Color.black.opacity(0.7))

                Spacer()

                // Close button at bottom
                Button {
                    showFullScreenMap = false
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "xmark.circle.fill")
                        Text("Close Map")
                    }
                    .font(.title3)
                    .padding(.horizontal, 45)
                    .padding(.vertical, 22)
                    .background(Color.white.opacity(0.3))
                    .cornerRadius(15)
                }
                .buttonStyle(.card)
                .padding(.bottom, 50)
            }
            .focusSection()
        }
        .transition(.opacity)
    }

    private var distanceString: String {
        let distance = haversineDistance(
            lat1: referenceCity.lat, lng1: referenceCity.lng,
            lat2: city.lat, lng2: city.lng
        )
        if distance < 1 {
            return String(format: "%.0f m", distance * 1000)
        } else if distance < 10 {
            return String(format: "%.2f km", distance)
        } else {
            return String(format: "%.1f km", distance)
        }
    }

    private func haversineDistance(lat1: Double, lng1: Double, lat2: Double, lng2: Double) -> Double {
        let R = 6371.0
        let dLat = (lat2 - lat1) * .pi / 180
        let dLng = (lng2 - lng1) * .pi / 180
        let a = sin(dLat/2) * sin(dLat/2) +
                cos(lat1 * .pi / 180) * cos(lat2 * .pi / 180) *
                sin(dLng/2) * sin(dLng/2)
        let c = 2 * atan2(sqrt(a), sqrt(1-a))
        return R * c
    }

    private func flagEmoji(for countryCode: String) -> String {
        let base: UInt32 = 127397
        var emoji = ""
        for scalar in countryCode.uppercased().unicodeScalars {
            if let unicode = UnicodeScalar(base + scalar.value) {
                emoji.append(String(unicode))
            }
        }
        return emoji.isEmpty ? "🌍" : emoji
    }

    private func formatPopulation(_ pop: UInt64) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        return formatter.string(from: NSNumber(value: pop)) ?? "\(pop)"
    }
}

// MARK: - Detail Item
struct DetailItem: View {
    let icon: String
    let title: String
    let value: String
    let color: Color

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: icon)
                .font(.system(size: 28))
                .foregroundColor(color)

            Text(title)
                .font(.body)
                .foregroundColor(.secondary)

            Text(value)
                .font(.body)
                .fontWeight(.semibold)
                .lineLimit(2)
                .minimumScaleFactor(0.6)
                .multilineTextAlignment(.center)
        }
        .frame(width: 200)
    }
}

// MARK: - CityResult ID Extension
extension CityResult: @retroactive Identifiable {
    public var id: String {
        "\(name)-\(lat)-\(lng)"
    }
}

#Preview {
    ContentView()
        .environmentObject(GeoDatabase())
}
#endif
