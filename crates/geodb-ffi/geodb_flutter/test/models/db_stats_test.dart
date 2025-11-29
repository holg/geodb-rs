import 'package:flutter_test/flutter_test.dart';
import 'package:geodb_flutter/models/models.dart';

void main() {
  group('DbStats', () {
    test('creates from valid map', () {
      final map = {
        'countries': 250,
        'states': 4874,
        'cities': 148249,
      };

      final stats = DbStats.fromMap(map);

      expect(stats.countries, 250);
      expect(stats.states, 4874);
      expect(stats.cities, 148249);
    });

    test('creates from map with zero values', () {
      final map = {
        'countries': 0,
        'states': 0,
        'cities': 0,
      };

      final stats = DbStats.fromMap(map);

      expect(stats.countries, 0);
      expect(stats.states, 0);
      expect(stats.cities, 0);
    });

    test('converts to map', () {
      final stats = DbStats(
        countries: 250,
        states: 4874,
        cities: 148249,
      );

      final map = stats.toMap();

      expect(map['countries'], 250);
      expect(map['states'], 4874);
      expect(map['cities'], 148249);
    });

    test('toString includes all counts', () {
      final stats = DbStats(
        countries: 250,
        states: 4874,
        cities: 148249,
      );

      final str = stats.toString();

      expect(str, contains('250'));
      expect(str, contains('4874'));
      expect(str, contains('148249'));
    });

    test('can access individual counts', () {
      final stats = DbStats(
        countries: 250,
        states: 4874,
        cities: 148249,
      );

      expect(stats.countries, isPositive);
      expect(stats.states, isPositive);
      expect(stats.cities, isPositive);
    });
  });
}
