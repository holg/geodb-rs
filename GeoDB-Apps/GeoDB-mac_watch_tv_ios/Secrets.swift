//
//  Secrets.swift
//  GeoDB
//
//  Environment variable loader for secure configuration
//  This file is committed to git. The .env file it loads is gitignored.
//

import Foundation

enum Secrets {

    // MARK: - Environment Variables

    /// API key for external services
    static var apiKey: String? {
        return get("API_KEY")
    }

    /// Google Maps API key (if used)
    static var googleMapsApiKey: String? {
        return get("GOOGLE_MAPS_API_KEY")
    }

    /// Development Team ID (for debugging)
    static var developmentTeamId: String? {
        return get("DEVELOPMENT_TEAM_ID")
    }

    /// Bundle Identifier (for debugging)
    static var bundleIdentifier: String? {
        return get("PRODUCT_BUNDLE_IDENTIFIER")
    }

    // MARK: - Feature Flags

    static var isAnalyticsEnabled: Bool {
        return getBool("ENABLE_ANALYTICS", default: false)
    }

    static var isCrashReportingEnabled: Bool {
        return getBool("ENABLE_CRASH_REPORTING", default: false)
    }

    // MARK: - Private Implementation

    private static var environment: [String: String] = {
        loadEnvironment()
    }()

    private static func get(_ key: String) -> String? {
        // Check process environment first (set by Xcode scheme or system)
        if let value = ProcessInfo.processInfo.environment[key] {
            return value
        }
        // Fall back to .env file
        return environment[key]
    }

    private static func getBool(_ key: String, default defaultValue: Bool) -> Bool {
        guard let value = get(key)?.lowercased() else {
            return defaultValue
        }
        return ["true", "1", "yes", "on"].contains(value)
    }

    private static func loadEnvironment() -> [String: String] {
        var env: [String: String] = [:]

        // Search multiple possible locations for .env file
        let possiblePaths = [
            // Development: project root (5 levels up from bundle)
            Bundle.main.bundlePath + "/../../../../../.env",
            // Production: bundled resource
            Bundle.main.path(forResource: ".env", ofType: nil),
            // Documents directory fallback
            FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first?.appendingPathComponent(".env").path
        ].compactMap { $0 }

        for path in possiblePaths {
            if let contents = try? String(contentsOfFile: path, encoding: .utf8) {
                env = parseEnvFile(contents)
                #if DEBUG
                print("[Secrets] Loaded .env from: \(path)")
                #endif
                break
            }
        }

        #if DEBUG
        if env.isEmpty {
            print("[Secrets] Warning: No .env file found. Searched paths:")
            possiblePaths.forEach { print("  - \($0)") }
        }
        #endif

        return env
    }

    private static func parseEnvFile(_ contents: String) -> [String: String] {
        var env: [String: String] = [:]
        let lines = contents.components(separatedBy: .newlines)

        for line in lines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            // Skip empty lines and comments
            if trimmed.isEmpty || trimmed.hasPrefix("#") {
                continue
            }

            // Split on first = only
            let parts = trimmed.components(separatedBy: "=")
            guard parts.count >= 2 else { continue }

            let key = parts[0].trimmingCharacters(in: .whitespaces)
            let value = parts[1...].joined(separator: "=")
                .trimmingCharacters(in: .whitespaces)
                .trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))

            env[key] = value
        }

        return env
    }
}
