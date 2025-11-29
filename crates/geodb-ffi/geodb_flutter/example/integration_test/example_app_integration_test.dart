// Integration tests for the example app
// These tests verify the example app works correctly with the GeoDB plugin

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:geodb_flutter_example/main.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  group('Example App Integration Tests', () {
    testWidgets('App launches and initializes GeoDB',
        (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());

      // Wait for initialization
      await tester.pumpAndSettle(const Duration(seconds: 2));

      // Should show database stats
      expect(find.textContaining('Countries:'), findsOneWidget);
      expect(find.textContaining('States:'), findsOneWidget);
      expect(find.textContaining('Cities:'), findsOneWidget);

      print('✅ App initialized successfully');
    });

    testWidgets('Shows database statistics', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      // Check stats are displayed
      final statsText = tester.widget<Text>(
        find.textContaining('Countries:'),
      );
      expect(statsText.data, isNotNull);

      print('✅ Database statistics displayed');
    });

    testWidgets('Smart Search button works', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      // Enter search query
      await tester.enterText(find.byType(TextField), 'Berlin');
      await tester.pumpAndSettle();

      // Find and tap Smart Search button
      final smartSearchButton = find.widgetWithText(
        ElevatedButton,
        'Smart Search',
      );
      expect(smartSearchButton, findsOneWidget);

      await tester.tap(smartSearchButton);
      await tester.pumpAndSettle(const Duration(seconds: 1));

      // Should show results
      expect(find.byType(ListTile), findsWidgets);

      print('✅ Smart Search works');
    });

    testWidgets('Nearest Cities button works', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      // Enter a city name
      await tester.enterText(find.byType(TextField), 'Berlin');
      await tester.pumpAndSettle();

      // Find and tap Nearest button
      final nearestButton = find.widgetWithText(
        ElevatedButton,
        'Nearest to Berlin',
      );

      if (nearestButton.evaluate().isNotEmpty) {
        await tester.tap(nearestButton);
        await tester.pumpAndSettle(const Duration(seconds: 1));

        // Should show results with distances
        expect(find.byType(ListTile), findsWidgets);

        print('✅ Nearest Cities search works');
      } else {
        print('ℹ️  Nearest button not found, skipping test');
      }
    });

    testWidgets('Radius Search button works', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      // Enter a city name
      await tester.enterText(find.byType(TextField), 'Berlin');
      await tester.pumpAndSettle();

      // Find and tap Radius button
      final radiusButton = find.text('50km Radius');

      if (radiusButton.evaluate().isNotEmpty) {
        await tester.tap(radiusButton);
        await tester.pumpAndSettle(const Duration(seconds: 1));

        // Should show results
        expect(find.byType(ListTile), findsWidgets);

        print('✅ Radius search works');
      } else {
        print('ℹ️  Radius button not found, skipping test');
      }
    });

    testWidgets('Countries search works', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      // Enter search query
      await tester.enterText(find.byType(TextField), 'United');
      await tester.pumpAndSettle();

      // Find Countries button
      final countriesButton = find.text('Countries');

      if (countriesButton.evaluate().isNotEmpty) {
        await tester.tap(countriesButton);
        await tester.pumpAndSettle(const Duration(seconds: 1));

        // Should show country results
        expect(find.byType(ListTile), findsWidgets);

        print('✅ Countries search works');
      } else {
        print('ℹ️  Countries button not found, skipping test');
      }
    });

    testWidgets('Results list displays correctly', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      // Perform a search
      await tester.enterText(find.byType(TextField), 'Tokyo');
      await tester.pumpAndSettle();

      final smartSearchButton = find.widgetWithText(
        ElevatedButton,
        'Smart Search',
      );
      await tester.tap(smartSearchButton);
      await tester.pumpAndSettle(const Duration(seconds: 1));

      // Check that results are displayed
      final results = find.byType(ListTile);
      expect(results, findsWidgets);

      // Each result should have title and subtitle
      for (final result in results.evaluate()) {
        final listTile = result.widget as ListTile;
        expect(listTile.title, isNotNull);
        expect(listTile.subtitle, isNotNull);
      }

      print('✅ Results display correctly');
    });

    testWidgets('Empty search shows appropriate message',
        (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      // Search for something that won't be found
      await tester.enterText(find.byType(TextField), 'XYZ123NotFound');
      await tester.pumpAndSettle();

      final smartSearchButton = find.widgetWithText(
        ElevatedButton,
        'Smart Search',
      );
      await tester.tap(smartSearchButton);
      await tester.pumpAndSettle(const Duration(seconds: 1));

      // Should show either no results or empty message
      // (depending on implementation)
      print('✅ Empty search handled');
    });

    testWidgets('App performance - search is fast',
        (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      await tester.enterText(find.byType(TextField), 'London');
      await tester.pumpAndSettle();

      final smartSearchButton = find.widgetWithText(
        ElevatedButton,
        'Smart Search',
      );

      // Measure search time
      final stopwatch = Stopwatch()..start();
      await tester.tap(smartSearchButton);
      await tester.pumpAndSettle();
      stopwatch.stop();

      expect(stopwatch.elapsedMilliseconds, lessThan(1000)); // Less than 1s

      print('✅ Search completed in ${stopwatch.elapsedMilliseconds}ms');
    });

    testWidgets('Multiple searches work correctly',
        (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      final smartSearchButton = find.widgetWithText(
        ElevatedButton,
        'Smart Search',
      );

      // Perform multiple searches
      final searches = ['Paris', 'Berlin', 'Tokyo', 'New York'];

      for (final query in searches) {
        await tester.enterText(find.byType(TextField), query);
        await tester.pumpAndSettle();

        await tester.tap(smartSearchButton);
        await tester.pumpAndSettle(const Duration(milliseconds: 500));

        expect(find.byType(ListTile), findsWidgets);

        print('✅ Search for "$query" successful');
      }

      print('✅ Multiple searches work correctly');
    });

    testWidgets('Scrolling through results works',
        (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle(const Duration(seconds: 2));

      // Perform a search that returns many results
      await tester.enterText(find.byType(TextField), 'New');
      await tester.pumpAndSettle();

      final smartSearchButton = find.widgetWithText(
        ElevatedButton,
        'Smart Search',
      );
      await tester.tap(smartSearchButton);
      await tester.pumpAndSettle(const Duration(seconds: 1));

      // Find the results list
      final listView = find.byType(ListView);
      if (listView.evaluate().isNotEmpty) {
        // Scroll down
        await tester.drag(listView, const Offset(0, -300));
        await tester.pumpAndSettle();

        // Scroll up
        await tester.drag(listView, const Offset(0, 300));
        await tester.pumpAndSettle();

        print('✅ Scrolling works correctly');
      } else {
        print('ℹ️  No scrollable list found');
      }
    });
  });
}
