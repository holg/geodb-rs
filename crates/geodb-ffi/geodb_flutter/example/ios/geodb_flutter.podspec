Pod::Spec.new do |s|
  s.name             = 'geodb_flutter'
  s.version          = '0.0.1'
  s.summary          = 'Flutter plugin for geodb-ffi'
  s.homepage         = 'https://github.com/holg/geodb-rs'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'Trahe Consult <trahe@mac.com>' => '' }
  s.source           = { :path => '.' }

  s.source_files = 'Classes/**/*'
  s.dependency 'Flutter'
  s.platform = :ios, '13.0'

  s.vendored_frameworks = 'GeodbFfi.xcframework'
  s.static_framework = true
  s.swift_version = '5.0'

  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386'
  }
end
