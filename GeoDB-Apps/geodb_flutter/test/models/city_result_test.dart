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
        'geoid': '1619799960716482548',
      };

      final city = CityResult.fromMap(map);

      expect(city.name, 'Berlin');
      expect(city.state, 'Berlin');
      expect(city.country, 'Germany');
      expect(city.iso2, 'DE');
      expect(city.lat, 52.52);
      expect(city.lng, 13.405);
      expect(city.population, 3644826);
      expect(city.geoid, '1619799960716482548');
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
        'geoid': '16197284850123456789',
        'distanceKm': 24.5,
      };

      final city = CityResult.fromMap(map);

      expect(city.name, 'Potsdam');
      expect(city.distanceKm, 24.5);
    });

    test('handles geoid exceeding i64 max (Pevek, Russia)', () {
      // Pevek is in far northeastern Russia with coordinates that produce
      // a geoid > 9,223,372,036,854,775,807 (i64::MAX)
      // This tests that String handling works for large geoids
      final map = {
        'name': 'Pevek',
        'state': 'Chukotka',
        'country': 'Russia',
        'iso2': 'RU',
        'lat': 69.7,
        'lng': 170.31,
        'population': 4485,
        'geoid': '18251692136994866019', // Exceeds i64::MAX
      };

      final city = CityResult.fromMap(map);

      expect(city.name, 'Pevek');
      expect(city.geoid, '18251692136994866019');
      // Verify the geoid string represents a value > i64::MAX
      final geoidValue = BigInt.parse(city.geoid);
      final i64Max = BigInt.parse('9223372036854775807');
      expect(geoidValue > i64Max, isTrue);
    });

    test('handles geoid exceeding i64 max (Tiksi, Russia)', () {
      // Another far northern Russian city with large geoid
      final map = {
        'name': 'Tiksi',
        'state': 'Sakha Republic',
        'country': 'Russia',
        'iso2': 'RU',
        'lat': 71.64,
        'lng': 128.87,
        'population': 4537,
        'geoid': '17971592435046529644', // Exceeds i64::MAX
      };

      final city = CityResult.fromMap(map);

      expect(city.name, 'Tiksi');
      expect(city.geoid, '17971592435046529644');
      final geoidValue = BigInt.parse(city.geoid);
      final i64Max = BigInt.parse('9223372036854775807');
      expect(geoidValue > i64Max, isTrue);
    });

    test('handles geoid exceeding i64 max (Anadyr, Russia)', () {
      // Anadyr - easternmost city in Russia
      final map = {
        'name': 'Anadyr',
        'state': 'Chukotka',
        'country': 'Russia',
        'iso2': 'RU',
        'lat': 64.73,
        'lng': 177.51,
        'population': 15804,
        'geoid': '17866912143317885329', // Exceeds i64::MAX
      };

      final city = CityResult.fromMap(map);

      expect(city.name, 'Anadyr');
      expect(city.geoid, '17866912143317885329');
      final geoidValue = BigInt.parse(city.geoid);
      final i64Max = BigInt.parse('9223372036854775807');
      expect(geoidValue > i64Max, isTrue);
    });

    test('handles zero geoid for countries', () {
      final map = {
        'name': 'Germany',
        'state': '',
        'country': 'Germany',
        'iso2': 'DE',
        'lat': 51.0,
        'lng': 9.0,
        'population': 83000000,
        'geoid': '0',
      };

      final city = CityResult.fromMap(map);

      expect(city.geoid, '0');
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
        'geoid': '0',
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
        geoid: '16045678901234567890',
      );

      final map = city.toMap();

      expect(map['name'], 'Munich');
      expect(map['state'], 'Bavaria');
      expect(map['country'], 'Germany');
      expect(map['iso2'], 'DE');
      expect(map['lat'], 48.1351);
      expect(map['lng'], 11.582);
      expect(map['population'], 1471508);
      expect(map['geoid'], '16045678901234567890');
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
        geoid: '16312345678901234567',
        distanceKm: 255.3,
      );

      final map = city.toMap();

      expect(map['distanceKm'], 255.3);
      expect(map['geoid'], '16312345678901234567');
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
        geoid: '15876543210987654321',
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
        geoid: '1619799960716482548',
      );

      final city2 = CityResult(
        name: 'Berlin',
        state: 'Berlin',
        country: 'Germany',
        iso2: 'DE',
        lat: 52.52,
        lng: 13.405,
        population: 3644826,
        geoid: '1619799960716482548',
      );

      expect(city1, equals(city2));
      expect(city1.hashCode, equals(city2.hashCode));
    });

    test('roundtrip large geoid through toMap/fromMap', () {
      // Test that large geoids survive serialization roundtrip
      final original = CityResult(
        name: 'Pevek',
        state: 'Chukotka',
        country: 'Russia',
        iso2: 'RU',
        lat: 69.7,
        lng: 170.31,
        population: 4485,
        geoid: '18251692136994866019',
      );

      final map = original.toMap();
      final restored = CityResult.fromMap(map);

      expect(restored.geoid, original.geoid);
      expect(restored.geoid, '18251692136994866019');
    });
  });
}
