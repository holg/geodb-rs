// Comprehensive integration tests for GeoDB Flutter plugin
//
// These tests require:
// - The SPM package (GeodbKit) to be added to the Xcode project
// - Running on an actual iOS Simulator, macOS, or device
//
// Run with: flutter test integration_test/geodb_integration_test.dart

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:geodb_flutter/geodb_flutter.dart';
import 'package:geodb_flutter/models/models.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  group('GeoDB Plugin Integration Tests', () {
    late GeodbFlutter geodb;

    setUpAll(() {
      geodb = GeodbFlutter();
    });

    group('Initialization', () {
      testWidgets('initializes successfully', (WidgetTester tester) async {
        await geodb.initialize();
        // If no exception thrown, initialization succeeded
      });

      testWidgets('getPlatformVersion returns non-empty string',
          (WidgetTester tester) async {
        final version = await geodb.getPlatformVersion();
        expect(version, isNotNull);
        expect(version!.isNotEmpty, true);
        // Should contain either "iOS" or "macOS"
        expect(version.contains('iOS') || version.contains('macOS'), true);
      });
    });

    group('Database Statistics', () {
      testWidgets('getStats returns valid statistics',
          (WidgetTester tester) async {
        final stats = await geodb.getStats();

        expect(stats, isA<DbStats>());
        expect(stats.countries, greaterThan(0));
        expect(stats.states, greaterThan(0));
        expect(stats.cities, greaterThan(0));

        // Sanity checks based on known database size
        expect(stats.countries, greaterThan(200)); // At least 200 countries
        expect(stats.countries, lessThan(300)); // Less than 300 countries
        expect(stats.cities, greaterThan(100000)); // At least 100k cities

        print('Database stats: ${stats}');
      });

      testWidgets('getCountryCount returns valid count',
          (WidgetTester tester) async {
        final count = await geodb.getCountryCount();

        expect(count, greaterThan(0));
        expect(count, greaterThan(200)); // At least 200 countries
        expect(count, lessThan(300)); // Less than 300 countries

        print('Country count: $count');
      });
    });

    group('Country Search', () {
      testWidgets('findCountryByCode finds Germany',
          (WidgetTester tester) async {
        final germany = await geodb.findCountryByCode('DE');

        expect(germany, isNotNull);
        expect(germany!.iso2, 'DE');
        expect(germany.name, 'Germany');
        expect(germany.state, isEmpty); // Countries have empty state
        expect(germany.population, greaterThan(0));

        print('Found: ${germany}');
      });

      testWidgets('findCountryByCode finds United States',
          (WidgetTester tester) async {
        final us = await geodb.findCountryByCode('US');

        expect(us, isNotNull);
        expect(us!.iso2, 'US');
        expect(us.name, 'United States');
      });

      testWidgets('findCountryByCode returns null for invalid code',
          (WidgetTester tester) async {
        final invalid = await geodb.findCountryByCode('XX');
        expect(invalid, isNull);
      });

      testWidgets('findCountriesBySubstring finds countries with "United"',
          (WidgetTester tester) async {
        final countries = await geodb.findCountriesBySubstring('United');

        expect(countries, isNotEmpty);
        expect(countries.length, greaterThanOrEqualTo(3));

        // Should find United States, United Kingdom, United Arab Emirates
        final names = countries.map((c) => c.name).toList();
        expect(names.any((name) => name.contains('United')), true);

        print('Found ${countries.length} countries with "United"');
        for (final country in countries.take(5)) {
          print('  - ${country.name} (${country.iso2})');
        }
      });

      testWidgets('findCountriesBySubstring is case-insensitive',
          (WidgetTester tester) async {
        final upper = await geodb.findCountriesBySubstring('GERMANY');
        final lower = await geodb.findCountriesBySubstring('germany');
        final mixed = await geodb.findCountriesBySubstring('GeRmAnY');

        expect(upper, isNotEmpty);
        expect(lower, isNotEmpty);
        expect(mixed, isNotEmpty);
      });
    });

    group('State Search', () {
      testWidgets('findStatesBySubstring finds California',
          (WidgetTester tester) async {
        final states = await geodb.findStatesBySubstring('California');

        expect(states, isNotEmpty);
        final california = states.firstWhere(
          (s) => s.name == 'California' && s.country == 'United States',
          orElse: () => states.first,
        );
        expect(california.name, contains('California'));
        expect(california.state, isNotEmpty);

        print('Found: ${california}');
      });

      testWidgets('findStatesBySubstring finds multiple results',
          (WidgetTester tester) async {
        final states = await geodb.findStatesBySubstring('New');

        expect(states, isNotEmpty);
        expect(states.length, greaterThan(5));

        print('Found ${states.length} states with "New"');
        for (final state in states.take(5)) {
          print('  - ${state.name}, ${state.country}');
        }
      });
    });

    group('City Search', () {
      testWidgets('findCitiesBySubstring finds Berlin',
          (WidgetTester tester) async {
        final cities = await geodb.findCitiesBySubstring('Berlin');

        expect(cities, isNotEmpty);

        // Should find Berlin, Germany as the first/main result
        final berlinGermany = cities.firstWhere(
          (c) => c.name == 'Berlin' && c.country == 'Germany',
          orElse: () => cities.first,
        );
        expect(berlinGermany.name, 'Berlin');
        expect(berlinGermany.country, 'Germany');
        expect(berlinGermany.population, greaterThan(3000000));

        print('Found ${cities.length} cities named Berlin');
        for (final city in cities.take(5)) {
          print('  - ${city}');
        }
      });

      testWidgets('findCitiesBySubstring finds New York',
          (WidgetTester tester) async {
        final cities = await geodb.findCitiesBySubstring('New York');

        expect(cities, isNotEmpty);
        final nyc = cities.firstWhere(
          (c) => c.name == 'New York City' && c.country == 'United States',
          orElse: () => cities.first,
        );
        expect(nyc.country, 'United States');
      });
    });

    group('Smart Search', () {
      testWidgets('smartSearch finds cities by name',
          (WidgetTester tester) async {
        final results = await geodb.smartSearch('Tokyo');

        expect(results, isNotEmpty);
        final tokyo = results.first;
        expect(tokyo.name, contains('Tokyo'));
        expect(tokyo.country, 'Japan');
        expect(tokyo.population, greaterThan(10000000));

        print('Smart search for "Tokyo": ${results.first}');
      });

      testWidgets('smartSearch handles partial matches',
          (WidgetTester tester) async {
        final results = await geodb.smartSearch('Par');

        expect(results, isNotEmpty);
        // Should find Paris and other cities starting with "Par"
        final names = results.map((r) => r.name).toList();
        print('Smart search for "Par" found: ${names.take(5).join(', ')}');
      });

      testWidgets('smartSearch is case-insensitive',
          (WidgetTester tester) async {
        final upper = await geodb.smartSearch('LONDON');
        final lower = await geodb.smartSearch('london');

        expect(upper, isNotEmpty);
        expect(lower, isNotEmpty);
      });
    });

    group('Spatial Queries', () {
      // Berlin coordinates
      const berlinLat = 52.52;
      const berlinLng = 13.405;

      testWidgets('findNearest finds cities near Berlin',
          (WidgetTester tester) async {
        final nearest = await geodb.findNearest(
          lat: berlinLat,
          lng: berlinLng,
          count: 10,
        );

        expect(nearest, hasLength(10));

        // First result should be Berlin itself or very close
        final first = nearest.first;
        expect(first.distanceKm, isNotNull);
        expect(first.distanceKm!, lessThan(50)); // Within 50km

        // Distances should be sorted (ascending)
        for (var i = 0; i < nearest.length - 1; i++) {
          expect(nearest[i].distanceKm!, lessThanOrEqualTo(nearest[i + 1].distanceKm!));
        }

        print('Nearest 10 cities to Berlin:');
        for (final city in nearest) {
          print('  - ${city.name}: ${city.distanceKm!.toStringAsFixed(1)} km');
        }
      });

      testWidgets('findNearest handles different counts',
          (WidgetTester tester) async {
        final five = await geodb.findNearest(
          lat: berlinLat,
          lng: berlinLng,
          count: 5,
        );
        final twenty = await geodb.findNearest(
          lat: berlinLat,
          lng: berlinLng,
          count: 20,
        );

        expect(five, hasLength(5));
        expect(twenty, hasLength(20));
      });

      testWidgets('findInRadius finds cities within 50km of Berlin',
          (WidgetTester tester) async {
        final nearby = await geodb.findInRadius(
          lat: berlinLat,
          lng: berlinLng,
          radiusKm: 50.0,
        );

        expect(nearby, isNotEmpty);

        // All results should be within 50km
        for (final city in nearby) {
          expect(city.distanceKm, isNotNull);
          expect(city.distanceKm!, lessThanOrEqualTo(50.0));
        }

        print('Found ${nearby.length} cities within 50km of Berlin');
        for (final city in nearby.take(10)) {
          print('  - ${city.name}: ${city.distanceKm!.toStringAsFixed(1)} km');
        }
      });

      testWidgets('findInRadius handles different radii',
          (WidgetTester tester) async {
        final small = await geodb.findInRadius(
          lat: berlinLat,
          lng: berlinLng,
          radiusKm: 10.0,
        );
        final large = await geodb.findInRadius(
          lat: berlinLat,
          lng: berlinLng,
          radiusKm: 100.0,
        );

        expect(small.length, lessThan(large.length));
        print('10km radius: ${small.length} cities, 100km radius: ${large.length} cities');
      });

      testWidgets('findInRadius returns empty for very small radius',
          (WidgetTester tester) async {
        // In the middle of ocean
        final ocean = await geodb.findInRadius(
          lat: 0.0,
          lng: 0.0,
          radiusKm: 10.0,
        );

        // Might be empty or very few results
        expect(ocean, isA<List<CityResult>>());
        print('Cities near 0,0 within 10km: ${ocean.length}');
      });
    });

    group('Performance', () {
      testWidgets('initialization is fast', (WidgetTester tester) async {
        final geodb2 = GeodbFlutter();
        final stopwatch = Stopwatch()..start();
        await geodb2.initialize();
        stopwatch.stop();

        expect(stopwatch.elapsedMilliseconds, lessThan(500)); // Less than 500ms
        print('Initialization took: ${stopwatch.elapsedMilliseconds}ms');
      });

      testWidgets('smart search is fast', (WidgetTester tester) async {
        final stopwatch = Stopwatch()..start();
        await geodb.smartSearch('Berlin');
        stopwatch.stop();

        expect(stopwatch.elapsedMilliseconds, lessThan(100)); // Less than 100ms
        print('Smart search took: ${stopwatch.elapsedMilliseconds}ms');
      });

      testWidgets('spatial query is fast', (WidgetTester tester) async {
        final stopwatch = Stopwatch()..start();
        await geodb.findNearest(lat: 52.52, lng: 13.405, count: 10);
        stopwatch.stop();

        expect(stopwatch.elapsedMilliseconds, lessThan(200)); // Less than 200ms
        print('findNearest took: ${stopwatch.elapsedMilliseconds}ms');
      });
    });

    group('Edge Cases', () {
      testWidgets('handles empty search strings gracefully',
          (WidgetTester tester) async {
        final results = await geodb.smartSearch('');
        // Should return empty or handle gracefully
        expect(results, isA<List<CityResult>>());
      });

      testWidgets('handles special characters', (WidgetTester tester) async {
        final results = await geodb.smartSearch('São Paulo');
        expect(results, isA<List<CityResult>>());
      });

      testWidgets('handles extreme coordinates', (WidgetTester tester) async {
        final north = await geodb.findNearest(lat: 89.0, lng: 0.0, count: 1);
        final south = await geodb.findNearest(lat: -89.0, lng: 0.0, count: 1);

        expect(north, isA<List<CityResult>>());
        expect(south, isA<List<CityResult>>());
      });

      testWidgets('handles very large radius', (WidgetTester tester) async {
        final results = await geodb.findInRadius(
          lat: 0.0,
          lng: 0.0,
          radiusKm: 20000.0, // Half the Earth
        );

        expect(results, isNotEmpty);
        expect(results.length, greaterThan(1000)); // Should find many cities
      });
    });
  });
}
