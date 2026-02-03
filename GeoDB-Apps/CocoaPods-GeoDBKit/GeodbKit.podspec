Pod::Spec.new do |spec|
  spec.name          = 'GeodbKit'
  spec.version       = '0.1.4'
  spec.summary       = 'High-performance geographic database for iOS - powered by Rust.'
  spec.description   = <<-DESC
    GeodbKit provides fast lookups of countries, states/regions, and cities.
    Built with Rust and distributed as a pre-compiled XCFramework.

    Features:
    - Fast city/state/country search by name
    - Spatial queries (nearest N, within radius)
    - Smart search combining multiple entity types
    - Embedded database (no network required)
  DESC

  spec.homepage      = 'https://github.com/holg/geodb-rs'
  spec.license       = { :type => 'MIT', :file => 'LICENSE' }
  spec.author        = { 'GeoDB Contributors' => 'https://github.com/holg/geodb-rs' }

  # For local path-based installation, use :path => in Podfile
  spec.source        = { :path => '.' }

  # iOS only (arm64 device and simulator)
  # Note: macOS, tvOS, watchOS require different XCFramework slices
  spec.ios.deployment_target = '13.0'

  spec.swift_version = '5.0'

  # Source files - Swift bindings
  spec.source_files = 'Sources/GeodbKit/**/*.swift'

  # XCFramework containing the Rust FFI library (iOS-only, dynamic)
  spec.vendored_frameworks = 'GeodbFfi-iOS.xcframework'

  # Module configuration
  spec.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'SWIFT_EMIT_LOC_STRINGS' => 'YES',
    # Only arm64 simulators (Apple Silicon Macs) are supported
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386 x86_64',
    'OTHER_LDFLAGS' => '$(inherited) -framework GeodbFfi'
  }

  spec.user_target_xcconfig = {
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386 x86_64'
  }

  # Required frameworks
  spec.frameworks = 'Foundation'

  # Preserve XCFramework structure
  spec.preserve_paths = 'GeodbFfi-iOS.xcframework'
end
