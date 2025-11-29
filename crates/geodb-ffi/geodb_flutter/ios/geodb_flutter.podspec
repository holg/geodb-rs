Pod::Spec.new do |spec|
  # -------------------------------------------------------------------------
  # Metadata
  # -------------------------------------------------------------------------
  spec.name          = 'geodb_flutter'
  spec.version       = '0.0.1'
  spec.summary       = 'Rust-powered core GeoDB library.'
  spec.description   = <<-DESC
    This Pod integrates the geodb-ffi Rust crate into iOS applications.
    Uses Swift Package Manager for the underlying Rust XCFramework.
  DESC

  spec.homepage      = 'https://github.com/holg/geodb-rs'
  spec.license       = { :type => 'MIT', :file => '../LICENSE' }
  spec.author        = { 'Trahe Consult <trahe@mac.com>' => '' }
  spec.source        = { :path => '.' }

  # -------------------------------------------------------------------------
  # Platform Requirements
  # -------------------------------------------------------------------------
  spec.ios.deployment_target = '13.0'
  spec.swift_version = '5.0'

  # -------------------------------------------------------------------------
  # Source Files
  # -------------------------------------------------------------------------
  # Include Flutter plugin Swift code
  spec.source_files = 'Classes/**/*.swift'

  # -------------------------------------------------------------------------
  # Dependencies
  # -------------------------------------------------------------------------
  spec.dependency 'Flutter'

  # -------------------------------------------------------------------------
  # Build Configuration
  # -------------------------------------------------------------------------
  spec.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386'
  }

  # -------------------------------------------------------------------------
  # Prepare Command - Add SPM Package to Xcode Project
  # -------------------------------------------------------------------------
  # This runs before pod install integrates the pod
  spec.prepare_command = <<-CMD
    echo "NOTE: GeodbFfi uses Swift Package Manager"
    echo "The SPM package must be added manually to the Xcode project:"
    echo "  1. Open Runner.xcworkspace in Xcode"
    echo "  2. File > Add Packages..."
    echo "  3. Choose 'Add Local...' and select: #{File.expand_path('../../geodb-ffi', __dir__)}"
    echo "  4. Add to Runner target"
  CMD
end
