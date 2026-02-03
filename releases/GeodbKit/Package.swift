// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "GeodbKit",
    platforms: [
        .iOS(.v13),
        .macOS(.v13),
        .tvOS(.v13),
        .watchOS(.v6),
        .visionOS(.v1),
    ],
    products: [
        .library(
            name: "GeodbKit",
            targets: ["GeodbKit"]
        ),
    ],
    targets: [
        // Binary FFI target - the framework module name is GeodbFfi
        // For debug builds, replace path with "GeodbFfi-debug.xcframework"
        .binaryTarget(
            name: "GeodbFfi",
            path: "GeodbFfi.xcframework"
        ),

        // Swift bindings target
        .target(
            name: "GeodbKit",
            dependencies: ["GeodbFfi"],
            path: "Sources/GeodbKit"
        ),

        // Tests
        .testTarget(
            name: "GeodbFfiTests",
            dependencies: ["GeodbKit"],
            path: "Tests/GeodbFfiTests"
        ),
    ]
)
