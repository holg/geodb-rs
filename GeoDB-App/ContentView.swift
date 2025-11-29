import SwiftUI
import GeodbKit

struct ContentView: View {
    @EnvironmentObject var database: GeoDatabase
    @State private var searchText = ""
    @State private var searchResults: [CityResult] = []
    @State private var searchMode: SearchMode = .smart
    @State private var isSearching = false
    @State private var selectedCity: CityResult?
    @State private var showingDetail = false

    // Spatial search parameters
    @State private var spatialLat = "52.52"
    @State private var spatialLng = "13.405"
    @State private var nearestCount = "10"
    @State private var radiusKm = "50"

    enum SearchMode: String, CaseIterable {
        case smart = "Smart Search"
        case cities = "Cities"
        case states = "States"
        case countries = "Countries"
        case nearest = "Nearest"
        case radius = "Within Radius"
    }

    var body: some View {
        #if os(iOS)
        NavigationView {
            VStack(spacing: 0) {
                // Search controls
                VStack(spacing: 12) {
                    // Mode picker
                    Picker("Search Mode", selection: $searchMode) {
                        ForEach(SearchMode.allCases, id: \.self) { mode in
                            Text(mode.rawValue).tag(mode)
                        }
                    }
                    .pickerStyle(.segmented)

                    // Search bar or spatial inputs
                    if searchMode == .nearest || searchMode == .radius {
                        spatialSearchInputs
                    } else {
                        textSearchBar
                    }
                }
                .padding()
                .background(Color(.systemGroupedBackground))

                // Results
                if isSearching {
                    ProgressView("Searching...")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if searchResults.isEmpty && (!searchText.isEmpty || searchMode == .nearest || searchMode == .radius) {
                    emptyState
                } else {
                    resultsList
                }
            }
            .navigationTitle("GeoDB")
            .navigationBarTitleDisplayMode(.inline)
            .sheet(isPresented: $showingDetail) {
                if let city = selectedCity {
                    CityDetailView(
                        city: city,
                        spatialLat: $spatialLat,
                        spatialLng: $spatialLng,
                        searchMode: $searchMode
                    )
                }
            }
        }
        #else
        macOSView
        #endif
    }

    #if os(iOS)
    private var textSearchBar: some View {
        HStack(spacing: 12) {
            HStack {
                Image(systemName: "magnifyingglass")
                    .foregroundColor(.secondary)

                TextField("Search...", text: $searchText)
                    .autocapitalization(.none)
                    .disableAutocorrection(true)
                    .onSubmit {
                        performSearch()
                    }

                if !searchText.isEmpty {
                    Button(action: {
                        searchText = ""
                        searchResults = []
                    }) {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundColor(.secondary)
                    }
                }
            }
            .padding(8)
            .background(Color(.systemBackground))
            .cornerRadius(8)

            Button("Search") {
                performSearch()
            }
            .buttonStyle(.borderedProminent)
            .disabled(searchText.isEmpty)
        }
    }

    private var spatialSearchInputs: some View {
        VStack(spacing: 8) {
            HStack {
                Text("Latitude:")
                    .frame(width: 70, alignment: .leading)
                TextField("52.52", text: $spatialLat)
                    .textFieldStyle(.roundedBorder)
                    .keyboardType(.decimalPad)
            }

            HStack {
                Text("Longitude:")
                    .frame(width: 70, alignment: .leading)
                TextField("13.405", text: $spatialLng)
                    .textFieldStyle(.roundedBorder)
                    .keyboardType(.decimalPad)
            }

            if searchMode == .nearest {
                HStack {
                    Text("Count:")
                        .frame(width: 70, alignment: .leading)
                    TextField("10", text: $nearestCount)
                        .textFieldStyle(.roundedBorder)
                        .keyboardType(.numberPad)
                }
            } else if searchMode == .radius {
                HStack {
                    Text("Radius (km):")
                        .frame(width: 70, alignment: .leading)
                    TextField("50", text: $radiusKm)
                        .textFieldStyle(.roundedBorder)
                        .keyboardType(.decimalPad)
                }
            }

            Button("Search") {
                performSearch()
            }
            .buttonStyle(.borderedProminent)
        }
        .font(.system(size: 14))
    }
    #endif

    private var macOSView: some View {
        NavigationSplitView {
            // Sidebar
            VStack(alignment: .leading, spacing: 20) {
                if let stats = database.stats {
                    statsView(stats)
                }

                Divider()

                Picker("Search Mode", selection: $searchMode) {
                    ForEach(SearchMode.allCases, id: \.self) { mode in
                        Text(mode.rawValue).tag(mode)
                    }
                }
                .pickerStyle(.inline)
                .labelsHidden()

                if searchMode == .nearest || searchMode == .radius {
                    Divider()
                    spatialSearchInputsMacOS
                }

                Spacer()
            }
            .padding()
            .frame(minWidth: 200)
        } detail: {
            VStack(spacing: 0) {
                if searchMode != .nearest && searchMode != .radius {
                    macOSSearchBar
                }

                if isSearching {
                    ProgressView("Searching...")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if searchResults.isEmpty && !searchText.isEmpty {
                    emptyState
                } else {
                    resultsList
                }
            }
        }
        .navigationTitle("GeoDB")
    }

    private var macOSSearchBar: some View {
        HStack {
            Image(systemName: "magnifyingglass")
                .foregroundColor(.secondary)

            TextField("Search cities, states, or countries...", text: $searchText)
                .textFieldStyle(.plain)
                .onSubmit {
                    performSearch()
                }

            if !searchText.isEmpty {
                Button(action: {
                    searchText = ""
                    searchResults = []
                }) {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
            }

            Button("Search") {
                performSearch()
            }
            .buttonStyle(.borderedProminent)
            .disabled(searchText.isEmpty)
        }
        .padding()
        .background(Color.gray.opacity(0.1))
    }

    private var spatialSearchInputsMacOS: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Spatial Search")
                .font(.headline)

            HStack {
                Text("Lat:")
                TextField("52.52", text: $spatialLat)
                    .textFieldStyle(.roundedBorder)
            }

            HStack {
                Text("Lng:")
                TextField("13.405", text: $spatialLng)
                    .textFieldStyle(.roundedBorder)
            }

            if searchMode == .nearest {
                HStack {
                    Text("Count:")
                    TextField("10", text: $nearestCount)
                        .textFieldStyle(.roundedBorder)
                }
            } else if searchMode == .radius {
                HStack {
                    Text("Radius:")
                    TextField("50", text: $radiusKm)
                        .textFieldStyle(.roundedBorder)
                }
            }

            Button("Search") {
                performSearch()
            }
            .buttonStyle(.borderedProminent)
        }
    }

    private var resultsList: some View {
        List(searchResults, id: \.self, selection: $selectedCity) { city in
            Button(action: {
                selectedCity = city
                #if os(iOS)
                showingDetail = true
                #endif
            }) {
                CityRowView(city: city)
            }
            .buttonStyle(.plain)
        }
        .listStyle(.inset)
    }

    private var emptyState: some View {
        VStack(spacing: 16) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 48))
                .foregroundColor(.secondary)

            Text("No results found")
                .font(.title2)
                .foregroundColor(.secondary)

            Text("Try a different search query")
                .font(.body)
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func statsView(_ stats: DbStatsDto) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Database Statistics")
                .font(.headline)

            HStack {
                Image(systemName: "flag.fill")
                    .foregroundColor(.blue)
                Text("\(stats.countries)")
                Text("Countries")
                    .foregroundColor(.secondary)
            }

            HStack {
                Image(systemName: "map.fill")
                    .foregroundColor(.green)
                Text("\(stats.states)")
                Text("States")
                    .foregroundColor(.secondary)
            }

            HStack {
                Image(systemName: "building.2.fill")
                    .foregroundColor(.orange)
                Text("\(stats.cities)")
                Text("Cities")
                    .foregroundColor(.secondary)
            }
        }
        .padding()
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(Color.gray.opacity(0.1))
        )
    }

    private func performSearch() {
        guard !searchText.isEmpty || searchMode == .nearest || searchMode == .radius else { return }

        isSearching = true

        Task {
            let results = await search(query: searchText, mode: searchMode)

            await MainActor.run {
                searchResults = results
                isSearching = false
            }
        }
    }

    private func search(query: String, mode: SearchMode) async -> [CityResult] {
        return await Task.detached {
            switch mode {
            case .smart:
                return await MainActor.run { database.smartSearch(query) }
            case .cities:
                return await MainActor.run { database.findCities(query) }
            case .states:
                return await MainActor.run { database.findStates(query) }
            case .countries:
                return await MainActor.run { database.findCountries(query) }
            case .nearest:
                guard let lat = Double(self.spatialLat),
                      let lng = Double(self.spatialLng),
                      let count = UInt32(self.nearestCount) else { return [] }
                return await MainActor.run {
                    database.findNearest(lat: lat, lng: lng, count: count)
                }
            case .radius:
                guard let lat = Double(self.spatialLat),
                      let lng = Double(self.spatialLng),
                      let radius = Double(self.radiusKm) else { return [] }
                return await MainActor.run {
                    database.findInRadius(lat: lat, lng: lng, radiusKm: radius)
                }
            }
        }.value
    }
}

struct CityRowView: View {
    let city: CityResult

    var body: some View {
        HStack(spacing: 12) {
            // Flag emoji
            if !city.iso2.isEmpty {
                Text(countryFlag(for: city.iso2))
                    .font(.system(size: 32))
            }

            VStack(alignment: .leading, spacing: 4) {
                Text(city.name)
                    .font(.headline)

                HStack(spacing: 4) {
                    if !city.state.isEmpty {
                        Text(city.state)
                            .font(.subheadline)
                            .foregroundColor(.secondary)
                        Text("•")
                            .foregroundColor(.secondary)
                    }

                    Text(city.country)
                        .font(.subheadline)
                        .foregroundColor(.secondary)

                    Text("•")
                        .foregroundColor(.secondary)

                    Text(city.iso2)
                        .font(.caption)
                        .padding(.horizontal, 4)
                        .padding(.vertical, 2)
                        .background(Color.blue.opacity(0.2))
                        .cornerRadius(4)
                }

                HStack {
                    Label("\(city.lat, specifier: "%.4f"), \(city.lng, specifier: "%.4f")",
                          systemImage: "location.fill")
                        .font(.caption)
                        .foregroundColor(.secondary)

                    Spacer()

                    if city.population > 0 {
                        Label("\(formattedPopulation(city.population))",
                              systemImage: "person.2.fill")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }

                    if let distance = city.distanceKm {
                        Label("\(distance, specifier: "%.1f") km",
                              systemImage: "arrow.left.and.right")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }
        }
        .padding(.vertical, 4)
    }

    private func formattedPopulation(_ pop: UInt64) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        return formatter.string(from: NSNumber(value: pop)) ?? "\(pop)"
    }

    private func countryFlag(for countryCode: String) -> String {
        let base: UInt32 = 127397
        var flag = ""
        for scalar in countryCode.uppercased().unicodeScalars {
            if let scalarValue = UnicodeScalar(base + scalar.value) {
                flag.append(String(scalarValue))
            }
        }
        return flag
    }
}

// Detail view for showing full city information
struct CityDetailView: View {
    let city: CityResult
    @Binding var spatialLat: String
    @Binding var spatialLng: String
    @Binding var searchMode: ContentView.SearchMode
    @Environment(\.dismiss) var dismiss

    @State private var coordinatesUpdated = false

    var body: some View {
        NavigationView {
            List {
                Section("Location") {
                    HStack {
                        Text(countryFlag(for: city.iso2))
                            .font(.system(size: 48))
                        VStack(alignment: .leading) {
                            Text(city.name)
                                .font(.title)
                            Text(city.country)
                                .foregroundColor(.secondary)
                        }
                    }
                    .padding(.vertical)
                }

                Section("Details") {
                    LabeledContent("State/Province", value: city.state.isEmpty ? "—" : city.state)
                    LabeledContent("Country", value: city.country)
                    LabeledContent("Country Code", value: city.iso2)
                    LabeledContent("Population", value: city.population > 0 ? formattedPopulation(city.population) : "—")
                }

                Section("Coordinates") {
                    LabeledContent("Latitude", value: String(format: "%.6f", city.lat))
                    LabeledContent("Longitude", value: String(format: "%.6f", city.lng))

                    if let distance = city.distanceKm {
                        LabeledContent("Distance", value: String(format: "%.1f km", distance))
                    }

                    if coordinatesUpdated {
                        HStack {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundColor(.green)
                            Text("Coordinates updated! Switch to Nearest or Radius mode to use them.")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                        .padding(.vertical, 4)
                    }

                    Button(action: {
                        spatialLat = String(format: "%.4f", city.lat)
                        spatialLng = String(format: "%.4f", city.lng)
                        coordinatesUpdated = true
                    }) {
                        Label("Use for Spatial Search", systemImage: "location.fill")
                    }
                }
            }
            .navigationTitle("Details")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .automatic) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    private func formattedPopulation(_ pop: UInt64) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        return formatter.string(from: NSNumber(value: pop)) ?? "\(pop)"
    }

    private func countryFlag(for countryCode: String) -> String {
        let base: UInt32 = 127397
        var flag = ""
        for scalar in countryCode.uppercased().unicodeScalars {
            if let scalarValue = UnicodeScalar(base + scalar.value) {
                flag.append(String(scalarValue))
            }
        }
        return flag
    }
}

#Preview {
    ContentView()
        .environmentObject(GeoDatabase())
}
