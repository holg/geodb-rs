import 'package:plugin_platform_interface/plugin_platform_interface.dart';

import 'geodb_flutter_method_channel.dart';
import 'models/models.dart';

abstract class GeodbFlutterPlatform extends PlatformInterface {
  /// Constructs a GeodbFlutterPlatform.
  GeodbFlutterPlatform() : super(token: _token);

  static final Object _token = Object();

  static GeodbFlutterPlatform _instance = MethodChannelGeodbFlutter();

  /// The default instance of [GeodbFlutterPlatform] to use.
  ///
  /// Defaults to [MethodChannelGeodbFlutter].
  static GeodbFlutterPlatform get instance => _instance;

  /// Platform-specific implementations should set this with their own
  /// platform-specific class that extends [GeodbFlutterPlatform] when
  /// they register themselves.
  static set instance(GeodbFlutterPlatform instance) {
    PlatformInterface.verifyToken(instance, _token);
    _instance = instance;
  }

  /// Initialize the GeoDB database
  Future<void> initialize() {
    throw UnimplementedError('initialize() has not been implemented.');
  }

  /// Get database statistics
  Future<DbStats> getStats() {
    throw UnimplementedError('getStats() has not been implemented.');
  }

  /// Get the number of countries in the database
  Future<int> getCountryCount() {
    throw UnimplementedError('getCountryCount() has not been implemented.');
  }

  /// Find a country by its ISO2 code
  Future<CityResult?> findCountryByCode(String code) {
    throw UnimplementedError('findCountryByCode() has not been implemented.');
  }

  /// Search for countries by substring
  Future<List<CityResult>> findCountriesBySubstring(String substring) {
    throw UnimplementedError('findCountriesBySubstring() has not been implemented.');
  }

  /// Search for states by substring
  Future<List<CityResult>> findStatesBySubstring(String substring) {
    throw UnimplementedError('findStatesBySubstring() has not been implemented.');
  }

  /// Search for cities by substring
  Future<List<CityResult>> findCitiesBySubstring(String substring) {
    throw UnimplementedError('findCitiesBySubstring() has not been implemented.');
  }

  /// Find nearest cities to a given location
  Future<List<CityResult>> findNearest({
    required double lat,
    required double lng,
    required int count,
  }) {
    throw UnimplementedError('findNearest() has not been implemented.');
  }

  /// Find cities within a radius
  Future<List<CityResult>> findInRadius({
    required double lat,
    required double lng,
    required double radiusKm,
  }) {
    throw UnimplementedError('findInRadius() has not been implemented.');
  }

  /// Smart search - searches across cities, states, and countries
  Future<List<CityResult>> smartSearch(String query) {
    throw UnimplementedError('smartSearch() has not been implemented.');
  }

  /// Get platform version (for testing)
  Future<String?> getPlatformVersion() {
    throw UnimplementedError('platformVersion() has not been implemented.');
  }
}
