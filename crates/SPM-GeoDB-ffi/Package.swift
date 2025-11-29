// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "GeodbKit",
    platforms: [
        .iOS(.v13),
        .macOS(.v13),
        .watchOS(.v6),
    ],
    products: [
        .library(
            name: "GeodbKit",
            targets: ["GeodbKit"]
        ),
    ],
    targets: [
        // 1) Binary FFI target from the xcframework
        //    The *name* here defines the Swift module name.
        .binaryTarget(
            name: "geodb_ffiFFI",
            path: "GeodbFfi.xcframework"
        ),

        // 2) Swift bindings target (GeoDbEngine, CityResult, ...)
        .target(
            name: "GeodbKit",
            dependencies: ["geodb_ffiFFI"],
            path: "Sources/GeodbKit"
        ),

        // 3) Tests
        .testTarget(
            name: "GeodbFfiTests",
            dependencies: ["GeodbKit"],
            path: "Tests/GeodbFfiTests"
        ),
    ]
)