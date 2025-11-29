import Cocoa
import FlutterMacOS
import GeodbKit  // Import the SPM package

public class GeodbFlutterPlugin: NSObject, FlutterPlugin {
    private var geodb: GeoDbEngine?

    public static func register(with registrar: FlutterPluginRegistrar) {
        let channel = FlutterMethodChannel(name: "geodb_flutter", binaryMessenger: registrar.messenger)
        let instance = GeodbFlutterPlugin()
        registrar.addMethodCallDelegate(instance, channel: channel)
    }

    public func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        switch call.method {
        case "initialize":
            handleInitialize(result: result)

        case "getStats":
            handleGetStats(result: result)

        case "getCountryCount":
            handleGetCountryCount(result: result)

        case "findCountryByCode":
            guard let args = call.arguments as? [String: Any],
                  let code = args["code"] as? String else {
                result(FlutterError(code: "INVALID_ARGS", message: "Missing 'code'", details: nil))
                return
            }
            handleFindCountryByCode(code: code, result: result)

        case "findCountriesBySubstring":
            guard let args = call.arguments as? [String: Any],
                  let substring = args["substring"] as? String else {
                result(FlutterError(code: "INVALID_ARGS", message: "Missing 'substring'", details: nil))
                return
            }
            handleFindCountriesBySubstring(substring: substring, result: result)

        case "findStatesBySubstring":
            guard let args = call.arguments as? [String: Any],
                  let substring = args["substring"] as? String else {
                result(FlutterError(code: "INVALID_ARGS", message: "Missing 'substring'", details: nil))
                return
            }
            handleFindStatesBySubstring(substring: substring, result: result)

        case "findCitiesBySubstring":
            guard let args = call.arguments as? [String: Any],
                  let substring = args["substring"] as? String else {
                result(FlutterError(code: "INVALID_ARGS", message: "Missing 'substring'", details: nil))
                return
            }
            handleFindCitiesBySubstring(substring: substring, result: result)

        case "findNearest":
            guard let args = call.arguments as? [String: Any],
                  let lat = args["lat"] as? Double,
                  let lng = args["lng"] as? Double,
                  let count = args["count"] as? Int else {
                result(FlutterError(code: "INVALID_ARGS", message: "Missing lat, lng, or count", details: nil))
                return
            }
            handleFindNearest(lat: lat, lng: lng, count: count, result: result)

        case "findInRadius":
            guard let args = call.arguments as? [String: Any],
                  let lat = args["lat"] as? Double,
                  let lng = args["lng"] as? Double,
                  let radiusKm = args["radiusKm"] as? Double else {
                result(FlutterError(code: "INVALID_ARGS", message: "Missing lat, lng, or radiusKm", details: nil))
                return
            }
            handleFindInRadius(lat: lat, lng: lng, radiusKm: radiusKm, result: result)

        case "smartSearch":
            guard let args = call.arguments as? [String: Any],
                  let query = args["query"] as? String else {
                result(FlutterError(code: "INVALID_ARGS", message: "Missing 'query'", details: nil))
                return
            }
            handleSmartSearch(query: query, result: result)

        case "getPlatformVersion":
            result("macOS " + ProcessInfo.processInfo.operatingSystemVersionString)

        default:
            result(FlutterMethodNotImplemented)
        }
    }

    // MARK: - Handler Methods

    private func handleInitialize(result: @escaping FlutterResult) {
        do {
            geodb = try GeoDbEngine()
            result(nil)
        } catch {
            result(FlutterError(code: "INIT_ERROR",
                              message: "Failed to initialize GeoDB",
                              details: error.localizedDescription))
        }
    }

    private func handleGetStats(result: @escaping FlutterResult) {
        guard let geodb = geodb else {
            result(FlutterError(code: "NOT_INITIALIZED", message: "GeoDB not initialized", details: nil))
            return
        }

        let stats = geodb.stats()
        result([
            "countries": stats.countries,
            "states": stats.states,
            "cities": stats.cities
        ])
    }

    private func handleGetCountryCount(result: @escaping FlutterResult) {
        guard let geodb = geodb else {
            result(FlutterError(code: "NOT_INITIALIZED", message: "GeoDB not initialized", details: nil))
            return
        }

        result(Int(geodb.countryCount()))
    }

    private func handleFindCountryByCode(code: String, result: @escaping FlutterResult) {
        guard let geodb = geodb else {
            result(FlutterError(code: "NOT_INITIALIZED", message: "GeoDB not initialized", details: nil))
            return
        }

        if let country = geodb.findCountryByCode(code: code) {
            result(cityResultToMap(country))
        } else {
            result(nil)
        }
    }

    private func handleFindCountriesBySubstring(substring: String, result: @escaping FlutterResult) {
        guard let geodb = geodb else {
            result(FlutterError(code: "NOT_INITIALIZED", message: "GeoDB not initialized", details: nil))
            return
        }

        let countries = geodb.findCountriesBySubstring(substr: substring)
        result(countries.map { cityResultToMap($0) })
    }

    private func handleFindStatesBySubstring(substring: String, result: @escaping FlutterResult) {
        guard let geodb = geodb else {
            result(FlutterError(code: "NOT_INITIALIZED", message: "GeoDB not initialized", details: nil))
            return
        }

        let states = geodb.findStatesBySubstring(substr: substring)
        result(states.map { cityResultToMap($0) })
    }

    private func handleFindCitiesBySubstring(substring: String, result: @escaping FlutterResult) {
        guard let geodb = geodb else {
            result(FlutterError(code: "NOT_INITIALIZED", message: "GeoDB not initialized", details: nil))
            return
        }

        let cities = geodb.findCitiesBySubstring(substr: substring)
        result(cities.map { cityResultToMap($0) })
    }

    private func handleFindNearest(lat: Double, lng: Double, count: Int, result: @escaping FlutterResult) {
        guard let geodb = geodb else {
            result(FlutterError(code: "NOT_INITIALIZED", message: "GeoDB not initialized", details: nil))
            return
        }

        let cities = geodb.findNearest(lat: lat, lng: lng, count: UInt32(count))
        result(cities.map { cityResultToMap($0) })
    }

    private func handleFindInRadius(lat: Double, lng: Double, radiusKm: Double, result: @escaping FlutterResult) {
        guard let geodb = geodb else {
            result(FlutterError(code: "NOT_INITIALIZED", message: "GeoDB not initialized", details: nil))
            return
        }

        let cities = geodb.findInRadius(lat: lat, lng: lng, radiusKm: radiusKm)
        result(cities.map { cityResultToMap($0) })
    }

    private func handleSmartSearch(query: String, result: @escaping FlutterResult) {
        guard let geodb = geodb else {
            result(FlutterError(code: "NOT_INITIALIZED", message: "GeoDB not initialized", details: nil))
            return
        }

        let results = geodb.smartSearch(query: query)
        result(results.map { cityResultToMap($0) })
    }

    // MARK: - Helper Methods

    private func cityResultToMap(_ city: CityResult) -> [String: Any] {
        var map: [String: Any] = [
            "name": city.name,
            "state": city.state,
            "country": city.country,
            "iso2": city.iso2,
            "lat": city.lat,
            "lng": city.lng,
            "population": Int(city.population)
        ]

        if let distanceKm = city.distanceKm {
            map["distanceKm"] = distanceKm
        }

        return map
    }
}
