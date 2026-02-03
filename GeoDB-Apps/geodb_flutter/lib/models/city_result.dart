/// Represents a city, state, or country result from GeoDB
class CityResult {
  /// Name of the location
  final String name;

  /// State name (empty for countries)
  final String state;

  /// Country name
  final String country;

  /// ISO2 country code
  final String iso2;

  /// Latitude
  final double lat;

  /// Longitude
  final double lng;

  /// Population
  final int population;

  /// Distance in kilometers (only for spatial queries)
  final double? distanceKm;

  const CityResult({
    required this.name,
    required this.state,
    required this.country,
    required this.iso2,
    required this.lat,
    required this.lng,
    required this.population,
    this.distanceKm,
  });

  /// Create from JSON map
  factory CityResult.fromMap(Map<String, dynamic> map) {
    return CityResult(
      name: map['name'] as String,
      state: map['state'] as String,
      country: map['country'] as String,
      iso2: map['iso2'] as String,
      lat: (map['lat'] as num).toDouble(),
      lng: (map['lng'] as num).toDouble(),
      population: map['population'] as int,
      distanceKm: map['distanceKm'] != null
          ? (map['distanceKm'] as num).toDouble()
          : null,
    );
  }

  /// Convert to JSON map
  Map<String, dynamic> toMap() {
    return {
      'name': name,
      'state': state,
      'country': country,
      'iso2': iso2,
      'lat': lat,
      'lng': lng,
      'population': population,
      if (distanceKm != null) 'distanceKm': distanceKm,
    };
  }

  @override
  String toString() {
    final location = state.isNotEmpty ? '$name, $state, $country' : '$name, $country';
    final distance = distanceKm != null ? ' (${distanceKm!.toStringAsFixed(1)} km)' : '';
    return '$location$distance';
  }

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    return other is CityResult &&
        other.name == name &&
        other.state == state &&
        other.country == country &&
        other.iso2 == iso2;
  }

  @override
  int get hashCode {
    return Object.hash(name, state, country, iso2);
  }
}
