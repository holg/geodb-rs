#!/usr/bin/env zsh
# scripts/sequential_flutter.sh
#
#————————————————————
#
#1. Generate Swift bindings with uniffi-bindgen
#
#————————————————————
#
#This uses your geodb.udl.
#pwd
#/Users/htr/Documents/develeop/rust/geodb-rs
rm -rf crates/geodb-ffi/generated crates/geodb-ffi/geodb_flutter crates/geodb-ffi/GeodbFfi.xcframework
cd crates/geodb-ffi
# later we shall add --release
cargo run --bin uniffi-bindgen -- \
    generate src/geodb.udl \
    --language swift \
    --out-dir generated/swift

#ls generated/swift
#geodb_ffi.swift        geodb_ffiFFI.h         geodb_ffiFFI.modulemap

#————————————————————
#
#2. Build Rust for iOS device (iphoneos)
#
#————————————————————


# Cpt. Obvious
#rustup target add aarch64-apple-ios
cargo build --release --target aarch64-apple-ios
#ls ../../target/aarch64-apple-ios/release
#build              deps               examples           incremental        libgeodb_ffi.a     libgeodb_ffi.d     libgeodb_ffi.dylib uniffi-bindgen     uniffi-bindgen.d

#————————————————————
#
#3. Build Rust for iOS simulator (iphonesimulator)
#
#————————————————————

#rustup target add aarch64-apple-ios-sim
cargo build --release --target aarch64-apple-ios-sim
#ls ../../target/aarch64-apple-ios-sim/release
#build              deps               examples           incremental        libgeodb_ffi.a     libgeodb_ffi.d     libgeodb_ffi.dylib uniffi-bindgen     uniffi-bindgen.d

#————————————————————
#
#4. Build XCFramework manually
#
#————————————————————

# remember these ../../ id bcs we are in the workspace, whose Cargo.toml is in ../../
# for the normal project, like done with cargo new, it would be simple target
xcodebuild -create-xcframework \
    -library ../../target/aarch64-apple-ios/release/libgeodb_ffi.a \
    -headers generated/swift \
    -library ../../target/aarch64-apple-ios-sim/release/libgeodb_ffi.a \
    -headers generated/swift \
    -output GeodbFfi.xcframework

#xcframework successfully written out to: /Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/GeodbFfi.xcframework
#ls GeodbFfi.xcframework
#Info.plist          ios-arm64           ios-arm64-simulator


#Verify architectures:
lipo -info GeodbFfi.xcframework/ios-arm64/libgeodb_ffi.a
lipo -info GeodbFfi.xcframework/ios-arm64-simulator/libgeodb_ffi.a
#Non-fat file: GeodbFfi.xcframework/ios-arm64/libgeodb_ffi.a is architecture: arm64
#Non-fat file: GeodbFfi.xcframework/ios-arm64-simulator/libgeodb_ffi.a is architecture: arm64


#————————————————————
#
#5. Create a fresh Flutter plugin from scratch
#
#————————————————————

flutter create --template=plugin --platforms=ios,android geodb_flutter
#Found saved certificate choice "Apple Development: Holger Trahe (xxx)". To clear, use "flutter config --clear-ios-signing-settings".
#Developer identity "Apple Development: Holger Trahe (xxx)" selected for iOS code signing
#Creating project geodb_flutter...
#Resolving dependencies in `geodb_flutter`...
#Downloading packages...
#Got dependencies in `geodb_flutter`.
#Resolving dependencies in `geodb_flutter/example`...
#Downloading packages...
#Got dependencies in `geodb_flutter/example`.
#Wrote 100 files.
#
#All done!
#
#Your plugin code is in geodb_flutter/lib/geodb_flutter.dart.
#
#Your example app code is in geodb_flutter/example/lib/main.dart.

#
#Host platform code is in the ios, android directories under geodb_flutter.
#To edit platform code in an IDE see https://flutter.dev/to/edit-plugins.
#
#
#To add platforms, run `flutter create -t plugin --platforms <platforms> .` under geodb_flutter.
#For more information, see https://flutter.dev/to/pubspec-plugin-platforms.

#————————————————————
#
#6. Install XCFramework + Swift bindings into plugin
#
#————————————————————
#6.1 Copy XCFramework
cp -R GeodbFfi.xcframework geodb_flutter/ios/
#6.2 Copy Swift bindings
cp generated/swift/*.swift geodb_flutter/ios/Classes/
cp generated/swift/*.h     geodb_flutter/ios/Classes/
cp generated/swift/*.modulemap geodb_flutter/ios/Classes/
 tree geodb_flutter/ios
#geodb_flutter/ios
#geodb_flutter/ios
 #├── Assets
 #├── Classes
 #│   ├── GeodbFlutterPlugin.swift
 #│   ├── geodb_ffi.swift
 #│   ├── geodb_ffiFFI.h
 #│   └── geodb_ffiFFI.modulemap
 #├── GeodbFfi.xcframework
 #│   ├── Info.plist
 #│   ├── ios-arm64
 #│   │   ├── Headers
 #│   │   │   ├── geodb_ffi.swift
 #│   │   │   ├── geodb_ffiFFI.h
 #│   │   │   └── geodb_ffiFFI.modulemap
 #│   │   └── libgeodb_ffi.a
 #│   └── ios-arm64-simulator
 #│       ├── Headers
 #│       │   ├── geodb_ffi.swift
 #│       │   ├── geodb_ffiFFI.h
 #│       │   └── geodb_ffiFFI.modulemap
 #│       └── libgeodb_ffi.a
 #├── Resources
 #│   └── PrivacyInfo.xcprivacy
 #└── geodb_flutter.podspec

#9 directories, 15 files

#————————————————————
#
#7. Fix Podspec
#
#————————————————————

#Edit:
#geodb_flutter/ios/geodb_flutter.podspec:
#______________________
#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint geodb_flutter.podspec` to validate before publishing.
#

#Pod::Spec.new do |s|
#  s.name             = 'geodb_flutter'
#  s.version          = '0.0.1'
#  s.summary          = 'Flutter plugin for geodb-ffi'
#  s.description      = <<-DESC
#A new Flutter plugin project.
#                       DESC
#  s.homepage         = 'https://github.com/holg/geodb-rs'
#  s.license          = { :file => '../LICENSE' }
#  s.author           = { 'Trahe Consult <trahe@mac.com>' => '' }
#  s.source           = { :path => '.' }
#  s.source_files = 'Classes/**/*'
#  s.dependency 'Flutter'
#  s.platform = :ios, '13.0'
#
#  # Flutter.framework does not contain a i386 slice.
#  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES', 'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386' }
#  s.swift_version = '5.0'

  # If your plugin requires a privacy manifest, for example if it uses any
  # required reason APIs, update the PrivacyInfo.xcprivacy file to describe your
  # plugin's privacy impact, and then uncomment this line. For more information,
  # see https://developer.apple.com/documentation/bundleresources/privacy_manifest_files
  # s.resource_bundles = {'geodb_flutter_privacy' => ['Resources/PrivacyInfo.xcprivacy']}
#end
#_____________________________________

# Ensure
# s.vendored_frameworks = 'GeodbFfi.xcframework'
#s.static_framework = true
#s.platform = :ios, '13.0'
#s.swift_version = '5.0'



# After Edit
#_____________________________________
#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint geodb_flutter.podspec` to validate before publishing.
#
#Pod::Spec.new do |s|
#  s.name             = 'geodb_flutter'
#  s.version          = '0.0.1'
#  s.summary          = 'Flutter plugin for geodb-ffi'
#  s.description      = <<-DESC
#A new Flutter plugin project.
#                       DESC
#  s.homepage         = 'https://github.com/holg/geodb-rs'
#  s.license          = { :file => '../LICENSE' }
#  s.author           = { 'Trahe Consult <trahe@mac.com>' => '' }
#  s.source           = { :path => '.' }
#  s.source_files = 'Classes/**/*'
#  s.dependency 'Flutter'
#  s.platform = :ios, '13.0'
#
#  # Flutter.framework does not contain a i386 slice.
#  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES', 'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386' }
#  s.swift_version = '5.0'
#  # htr
#  s.vendored_frameworks = 'GeodbFfi.xcframework'
#  s.static_framework = true
#  s.platform = :ios, '13.0'
#  s.swift_version = '5.0'
#  # /htr

  # If your plugin requires a privacy manifest, for example if it uses any
  # required reason APIs, update the PrivacyInfo.xcprivacy file to describe your
  # plugin's privacy impact, and then uncomment this line. For more information,
  # see https://developer.apple.com/documentation/bundleresources/privacy_manifest_files
  # s.resource_bundles = {'geodb_flutter_privacy' => ['Resources/PrivacyInfo.xcprivacy']}
#end
#_____________________________________
#IMPORTANT: No commas inside quotes except separating list items.

#————————————————————
#
#8. Create example Podfile
#
#————————————————————
#Go to example:
cd geodb_flutter/example/ios

# Create Podfile:

#platform :ios, '13.0'
#
#use_frameworks!
#use_modular_headers!
#
#target 'Runner' do
#  pod 'geodb_flutter', :path => '../..'
#end


#————————————————————
#
#9. Run pod install
#
#————————————————————

pod repo update
pod install

#Updating spec repo `trunk`
#➜  ios git:(refactor/flat-data-model) ✗ pod install
#Analyzing dependencies
#[!] No podspec found for `geodb_flutter` in `../..`
#Fixed #  pod 'geodb_flutter', :path => '../..'

pwd
#/Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter
tree -L 3
#.
#├── CHANGELOG.md
#├── LICENSE
#├── README.md
#├── analysis_options.yaml
#├── android
#│   ├── build.gradle
#│   ├── geodb_flutter_android.iml
#│   ├── local.properties
#│   ├── settings.gradle
#│   └── src
#│       ├── main
#│       └── test
#├── example
#│   ├── README.md
#│   ├── analysis_options.yaml
#│   ├── android
#│   │   ├── app
#│   │   ├── build.gradle.kts
#│   │   ├── geodb_flutter_example_android.iml
#│   │   ├── gradle
#│   │   ├── gradle.properties
#│   │   ├── gradlew
#│   │   ├── gradlew.bat
#│   │   ├── local.properties
#│   │   └── settings.gradle.kts
#│   ├── geodb_flutter_example.iml
#│   ├── integration_test
#│   │   └── plugin_integration_test.dart
#│   ├── ios
#│   │   ├── Flutter
#│   │   ├── Podfile
#│   │   ├── Podfile.lock
#│   │   ├── Pods
#│   │   ├── Runner
#│   │   ├── Runner.xcodeproj
#│   │   ├── Runner.xcworkspace
#│   │   └── RunnerTests
#│   ├── lib
#│   │   └── main.dart
#│   ├── pubspec.lock
#│   ├── pubspec.yaml
#│   └── test
#│       └── widget_test.dart
#├── geodb_flutter.iml
#├── ios
#│   ├── Assets
#│   ├── Classes
#│   │   ├── GeodbFlutterPlugin.swift
#│   │   ├── geodb_ffi.swift
#│   │   ├── geodb_ffiFFI.h
#│   │   └── geodb_ffiFFI.modulemap
#│   ├── GeodbFfi.xcframework
#│   │   ├── Info.plist
#│   │   ├── ios-arm64
#│   │   └── ios-arm64-simulator
#│   ├── Resources
#│   │   └── PrivacyInfo.xcprivacy
#│   └── geodb_flutter.podspec
#├── lib
#│   ├── geodb_flutter.dart
#│   ├── geodb_flutter_method_channel.dart
#│   └── geodb_flutter_platform_interface.dart
#├── pubspec.lock
#├── pubspec.yaml
#└── test
#    ├── geodb_flutter_method_channel_test.dart
#    └── geodb_flutter_test.dart
#
#28 directories, 40 files

#————————————————————
#
#10. Build iOS example app (manual Xcode test)
#
#————————————————————
open Runner.xcworkspace
# on build:

#Unable to read contents of XCFileList '/Target Support Files/Pods-Runner/Pods-Runner-frameworks-Debug-output-files.xcfilelist'
#
#Unable to load contents of file list: '/Target Support Files/Pods-Runner/Pods-Runner-frameworks-Debug-input-files.xcfilelist'
#
#Unable to load contents of file list: '/Target Support Files/Pods-Runner/Pods-Runner-frameworks-Debug-output-files.xcfilelist'
#
#Run script build phase '[CP] Embed Pods Frameworks' will be run during every build because it does not specify any outputs. To address this issue, either add output dependencies to the script phase, or configure it to run in every build by unchecking "Based on dependency analysis" in the script phase.
#
#/Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/example/ios/Flutter/Debug.xcconfig
#/Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/example/ios/Flutter/Debug.xcconfig:1:1 could not find included file 'Generated.xcconfig' in search paths
#
#geodb_flutter
#no rule to process file '/Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/ios/Classes/geodb_ffiFFI.modulemap' of type 'sourcecode.module-map' for architecture 'arm64'
#
#Flutter
#/Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/example/ios/Pods/Pods.xcodeproj The iOS deployment target 'IPHONEOS_DEPLOYMENT_TARGET' is set to 11.0, but the range of supported deployment target versions is 12.0 to 26.1.99.


#Fix
# 🔧 STEP 1 — Regenerate all Flutter iOS project scaffolding
#cd crates/geodb-ffi/geodb_flutter
flutter create --platforms=ios .
#Found saved certificate choice "Apple Development: Holger Trahe (49N7XTSGER)". To clear, use "flutter config --clear-ios-signing-settings".
#Developer identity "Apple Development: Holger Trahe (49N7XTSGER)" selected for iOS code signing
#Recreating project ....
#Resolving dependencies...
#Downloading packages...
#Got dependencies.
#Resolving dependencies in `./example`...
#Downloading packages...
#Got dependencies in `./example`.
#Wrote 3 files.
#
#All done!
#
#Your plugin code is in ./lib/geodb_flutter.dart.
#
#Your example app code is in ./example/lib/main.dart.
#
#
#Host platform code is in the ios directories under ..
#To edit platform code in an IDE see https://flutter.dev/to/edit-plugins.
#
#
#You need to update ./pubspec.yaml to support ios.
#
#
#
#To add platforms, run `flutter create -t plugin --platforms <platforms> .` under ..
#For more information, see https://flutter.dev/to/pubspec-plugin-platforms.

cd ios
#pwd
#/Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/example/ios
#🔧 STEP 3 — Fix your Podfile (this is important)
#geodb_flutter/example/ios/Podfile
cat """
platform :ios, '13.0'

use_frameworks!
use_modular_headers!

target 'Runner' do
  pod 'geodb_flutter', :path => '../../ios'
end
""" > Podfile

