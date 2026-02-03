Pod::Spec.new do |spec|
  spec.name          = 'geodb_flutter'
  spec.version       = '0.0.1'
  spec.summary       = 'Flutter plugin for GeoDB - geographic database powered by Rust.'
  spec.description   = <<-DESC
    Flutter plugin that provides access to GeoDB, a high-performance geographic
    database for searching cities, states, and countries. Powered by Rust via UniFFI.
  DESC

  spec.homepage      = 'https://github.com/holg/geodb-rs'
  spec.license       = { :type => 'MIT', :file => '../LICENSE' }
  spec.author        = { 'GeoDB Contributors' => 'https://github.com/holg/geodb-rs' }
  spec.source        = { :path => '.' }

  spec.platform = :osx, '10.13'
  spec.swift_version = '5.0'

  # Source files - plugin and UniFFI bindings
  spec.source_files = 'Classes/**/*.{swift,h}'

  # Public headers - exposes C types to Swift
  spec.public_header_files = 'Classes/geodb_ffiFFI.h'

  # Use macOS framework
  spec.vendored_frameworks = 'Frameworks/GeodbFfi.framework'

  spec.dependency 'FlutterMacOS'

  spec.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'OTHER_LDFLAGS' => '$(inherited) -framework GeodbFfi'
  }
end
