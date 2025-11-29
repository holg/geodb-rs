import SwiftUI
import GeodbKit

struct WatchContentView: View {
    @EnvironmentObject var database: GeoDatabase
    @State private var searchText = ""
    @State private var searchResults: [CityResult] = []
    @State private var isSearching = false
    @State private var selectedCity: CityResult?

    var body: some View {
        NavigationView {
            VStack {
                // Simple search interface
                TextField("Search city...", text: $searchText)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit {
                        performSearch()
                    }

                Button("Search") {
                    performSearch()
                }
                .buttonStyle(.borderedProminent)
                .disabled(searchText.isEmpty)

                if isSearching {
                    ProgressView("Searching...")
                } else if searchResults.isEmpty && !searchText.isEmpty {
                    Text("No results")
                        .foregroundColor(.secondary)
                        .font(.caption)
                } else {
                    List(searchResults.prefix(20), id: \.self) { city in
                        NavigationLink(destination: WatchCityDetailView(city: city)) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(city.name)
                                    .font(.headline)
                                Text(city.country)
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
            }
            .navigationTitle("GeoDB")
        }
    }

    private func performSearch() {
        guard !searchText.isEmpty else { return }

        isSearching = true

        Task {
            let results = await Task.detached {
                await MainActor.run { database.smartSearch(searchText) }
            }.value

            await MainActor.run {
                searchResults = results
                isSearching = false
            }
        }
    }
}

struct WatchCityDetailView: View {
    let city: CityResult

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                // City name and flag
                HStack {
                    if !city.iso2.isEmpty {
                        Text(countryFlag(for: city.iso2))
                            .font(.system(size: 32))
                    }
                    VStack(alignment: .leading) {
                        Text(city.name)
                            .font(.headline)
                        Text(city.country)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
                .padding(.bottom, 4)

                Divider()

                // Details
                if !city.state.isEmpty {
                    DetailRow(label: "State", value: city.state)
                }

                DetailRow(label: "Country", value: city.country)
                DetailRow(label: "Code", value: city.iso2)

                if city.population > 0 {
                    DetailRow(label: "Population", value: formattedPopulation(city.population))
                }

                Divider()

                // Coordinates
                VStack(alignment: .leading, spacing: 4) {
                    Text("Coordinates")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    Text(String(format: "%.4f, %.4f", city.lat, city.lng))
                        .font(.footnote)
                }

                if let distance = city.distanceKm {
                    Divider()
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Distance")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        Text(String(format: "%.1f km", distance))
                            .font(.footnote)
                    }
                }
            }
            .padding()
        }
        .navigationTitle("Details")
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

struct DetailRow: View {
    let label: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.caption)
                .foregroundColor(.secondary)
            Text(value)
                .font(.footnote)
        }
    }
}

extension CityResult: Hashable {
    public static func == (lhs: CityResult, rhs: CityResult) -> Bool {
        lhs.name == rhs.name && lhs.lat == rhs.lat && lhs.lng == rhs.lng
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(name)
        hasher.combine(lat)
        hasher.combine(lng)
    }
}
