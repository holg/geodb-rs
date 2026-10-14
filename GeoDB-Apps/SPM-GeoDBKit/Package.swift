// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "GeodbKit",
    platforms: [
        .iOS(.v13),
        .macOS(.v13),
        .tvOS(.v13),
        .watchOS(.v6),
    ],
    products: [
        .library(
            name: "GeodbKit",
            targets: ["GeodbKit"]
        ),
        // The 3D globe (wgpu on Metal) as a SwiftUI view; not on watchOS
        .library(
            name: "GeodbGlobe",
            targets: ["GeodbGlobe"]
        ),
        // The board's software globe for watchOS
        .library(
            name: "GeodbWatchGlobe",
            targets: ["GeodbWatchGlobe"]
        ),
    ],
    targets: [
        // Binary FFI target - the framework module name is GeodbFfi
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

        // The globe view over the C interface of crates/geodb-ffi/src/globe.rs
        .target(
            name: "GeodbGlobe",
            dependencies: ["GeodbFfi"],
            path: "Sources/GeodbGlobe",
            linkerSettings: [
                .linkedFramework("Metal"),
                .linkedFramework("QuartzCore"),
            ]
        ),

        // The watch globe view over the C interface of crates/geodb-ffi/src/watch_globe.rs
        .target(
            name: "GeodbWatchGlobe",
            dependencies: ["GeodbFfi"],
            path: "Sources/GeodbWatchGlobe"
        ),

        // Tests
        .testTarget(
            name: "GeodbFfiTests",
            dependencies: ["GeodbKit"],
            path: "Tests/GeodbFfiTests"
        ),
    ]
)
