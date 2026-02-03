import 'geodb_flutter_platform_interface.dart';
import 'models/models.dart';

export 'models/models.dart';

/// GeoDB Flutter Plugin
///
/// Provides access to the GeoDB database for searching cities, states, and countries.
///
/// Example:
/// ```dart
/// final geodb = GeodbFlutter();
///
/// // Initialize
/// await geodb.initialize();
///
/// // Search
/// final results = await geodb.smartSearch('Berlin');
/// for (final city in results) {
///   print('${city.name}, ${city.country}');
/// }
/// ```
class GeodbFlutter {
  /// Initialize the GeoDB database
  ///
  /// Must be called before using any other methods.
  /// This loads the embedded database into memory.
  Future<void> initialize() {
    return GeodbFlutterPlatform.instance.initialize();
  }

  /// Get database statistics
  ///
  /// Returns the number of countries, states, and cities in the database.
  Future<DbStats> getStats() {
    return GeodbFlutterPlatform.instance.getStats();
  }

  /// Get the total number of countries
  Future<int> getCountryCount() {
    return GeodbFlutterPlatform.instance.getCountryCount();
  }

  /// Find a country by its ISO2 code
  ///
  /// Example:
  /// ```dart
  /// final germany = await geodb.findCountryByCode('DE');
  /// print(germany?.name); // 'Germany'
  /// ```
  Future<CityResult?> findCountryByCode(String code) {
    return GeodbFlutterPlatform.instance.findCountryByCode(code);
  }

  /// Search for countries by substring
  ///
  /// Example:
  /// ```dart
  /// final results = await geodb.findCountriesBySubstring('United');
  /// // Returns: United States, United Kingdom, United Arab Emirates, etc.
  /// ```
  Future<List<CityResult>> findCountriesBySubstring(String substring) {
    return GeodbFlutterPlatform.instance.findCountriesBySubstring(substring);
  }

  /// Search for states/provinces by substring
  ///
  /// Example:
  /// ```dart
  /// final results = await geodb.findStatesBySubstring('California');
  /// ```
  Future<List<CityResult>> findStatesBySubstring(String substring) {
    return GeodbFlutterPlatform.instance.findStatesBySubstring(substring);
  }

  /// Search for cities by substring
  ///
  /// Example:
  /// ```dart
  /// final results = await geodb.findCitiesBySubstring('New York');
  /// ```
  Future<List<CityResult>> findCitiesBySubstring(String substring) {
    return GeodbFlutterPlatform.instance.findCitiesBySubstring(substring);
  }

  /// Find the nearest cities to a location
  ///
  /// [lat] - Latitude
  /// [lng] - Longitude
  /// [count] - Number of results to return
  ///
  /// Example:
  /// ```dart
  /// final nearest = await geodb.findNearest(
  ///   lat: 52.52,
  ///   lng: 13.405,
  ///   count: 10,
  /// );
  /// ```
  Future<List<CityResult>> findNearest({
    required double lat,
    required double lng,
    required int count,
  }) {
    return GeodbFlutterPlatform.instance.findNearest(
      lat: lat,
      lng: lng,
      count: count,
    );
  }

  /// Find cities within a radius
  ///
  /// [lat] - Center latitude
  /// [lng] - Center longitude
  /// [radiusKm] - Radius in kilometers
  ///
  /// Example:
  /// ```dart
  /// final nearby = await geodb.findInRadius(
  ///   lat: 52.52,
  ///   lng: 13.405,
  ///   radiusKm: 50.0,
  /// );
  /// ```
  Future<List<CityResult>> findInRadius({
    required double lat,
    required double lng,
    required double radiusKm,
  }) {
    return GeodbFlutterPlatform.instance.findInRadius(
      lat: lat,
      lng: lng,
      radiusKm: radiusKm,
    );
  }

  /// Smart search across cities, states, and countries
  ///
  /// This is the recommended method for general search as it searches
  /// across all types of locations and ranks results intelligently.
  ///
  /// Example:
  /// ```dart
  /// final results = await geodb.smartSearch('Berlin');
  /// // Returns: Berlin (city), Berlin (state), etc.
  /// ```
  Future<List<CityResult>> smartSearch(String query) {
    return GeodbFlutterPlatform.instance.smartSearch(query);
  }

  /// Get the platform version (for testing)
  Future<String?> getPlatformVersion() {
    return GeodbFlutterPlatform.instance.getPlatformVersion();
  }
}
