import 'package:flutter_test/flutter_test.dart';
import 'package:geodb_flutter/models/models.dart';

void main() {
  group('CityResult', () {
    test('creates from valid map', () {
      final map = {
        'name': 'Berlin',
        'state': 'Berlin',
        'country': 'Germany',
        'iso2': 'DE',
        'lat': 52.52,
        'lng': 13.405,
        'population': 3644826,
      };

      final city = CityResult.fromMap(map);

      expect(city.name, 'Berlin');
      expect(city.state, 'Berlin');
      expect(city.country, 'Germany');
      expect(city.iso2, 'DE');
      expect(city.lat, 52.52);
      expect(city.lng, 13.405);
      expect(city.population, 3644826);
      expect(city.distanceKm, isNull);
    });

    test('creates from map with distance', () {
      final map = {
        'name': 'Potsdam',
        'state': 'Brandenburg',
        'country': 'Germany',
        'iso2': 'DE',
        'lat': 52.3906,
        'lng': 13.0645,
        'population': 159456,
        'distanceKm': 24.5,
      };

      final city = CityResult.fromMap(map);

      expect(city.name, 'Potsdam');
      expect(city.distanceKm, 24.5);
    });

    test('handles empty state', () {
      final map = {
        'name': 'Germany',
        'state': '',
        'country': 'Germany',
        'iso2': 'DE',
        'lat': 51.0,
        'lng': 9.0,
        'population': 83000000,
      };

      final city = CityResult.fromMap(map);

      expect(city.state, isEmpty);
    });

    test('converts to map', () {
      final city = CityResult(
        name: 'Munich',
        state: 'Bavaria',
        country: 'Germany',
        iso2: 'DE',
        lat: 48.1351,
        lng: 11.582,
        population: 1471508,
      );

      final map = city.toMap();

      expect(map['name'], 'Munich');
      expect(map['state'], 'Bavaria');
      expect(map['country'], 'Germany');
      expect(map['iso2'], 'DE');
      expect(map['lat'], 48.1351);
      expect(map['lng'], 11.582);
      expect(map['population'], 1471508);
      expect(map.containsKey('distanceKm'), isFalse);
    });

    test('converts to map with distance', () {
      final city = CityResult(
        name: 'Hamburg',
        state: 'Hamburg',
        country: 'Germany',
        iso2: 'DE',
        lat: 53.5511,
        lng: 9.9937,
        population: 1841179,
        distanceKm: 255.3,
      );

      final map = city.toMap();

      expect(map['distanceKm'], 255.3);
    });

    test('toString includes location and distance', () {
      final city = CityResult(
        name: 'Frankfurt',
        state: 'Hesse',
        country: 'Germany',
        iso2: 'DE',
        lat: 50.1109,
        lng: 8.6821,
        population: 753056,
        distanceKm: 424.2,
      );

      final str = city.toString();

      expect(str, contains('Frankfurt'));
      expect(str, contains('Hesse'));
      expect(str, contains('Germany'));
      expect(str, contains('424.2'));
    });

    test('equality and hashCode', () {
      final city1 = CityResult(
        name: 'Berlin',
        state: 'Berlin',
        country: 'Germany',
        iso2: 'DE',
        lat: 52.52,
        lng: 13.405,
        population: 3644826,
      );

      final city2 = CityResult(
        name: 'Berlin',
        state: 'Berlin',
        country: 'Germany',
        iso2: 'DE',
        lat: 52.52,
        lng: 13.405,
        population: 3644826,
      );

      expect(city1, equals(city2));
      expect(city1.hashCode, equals(city2.hashCode));
    });
  });
}
