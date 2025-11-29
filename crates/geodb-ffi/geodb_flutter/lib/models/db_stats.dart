/// Database statistics
class DbStats {
  /// Number of countries in the database
  final int countries;

  /// Number of states/provinces in the database
  final int states;

  /// Number of cities in the database
  final int cities;

  const DbStats({
    required this.countries,
    required this.states,
    required this.cities,
  });

  /// Create from JSON map
  factory DbStats.fromMap(Map<String, dynamic> map) {
    return DbStats(
      countries: map['countries'] as int,
      states: map['states'] as int,
      cities: map['cities'] as int,
    );
  }

  /// Convert to JSON map
  Map<String, dynamic> toMap() {
    return {
      'countries': countries,
      'states': states,
      'cities': cities,
    };
  }

  @override
  String toString() {
    return 'DbStats(countries: $countries, states: $states, cities: $cities)';
  }
}
