import 'package:flutter_test/flutter_test.dart';
import 'package:geodb_flutter_example/main.dart';

void main() {
  group('Example App Widget Tests', () {
    testWidgets('App builds without crashing', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      expect(find.text('GeoDB Flutter Example'), findsOneWidget);
    });

    testWidgets('Shows initialization state', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pump();

      // Should show either loading or initialized state
      expect(
        find.textContaining('Initializing', findRichText: true).or(
          find.textContaining('Countries:', findRichText: true),
        ),
        findsWidgets,
      );
    });

    testWidgets('Has search field', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle();

      expect(find.byType(TextField), findsOneWidget);
    });

    testWidgets('Has action buttons', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle();

      // Should have multiple buttons for different search types
      expect(find.byType(ElevatedButton), findsWidgets);
    });

    testWidgets('Search field accepts input', (WidgetTester tester) async {
      await tester.pumpWidget(const MyApp());
      await tester.pumpAndSettle();

      // Find and tap the text field
      final textField = find.byType(TextField);
      await tester.enterText(textField, 'Berlin');

      expect(find.text('Berlin'), findsOneWidget);
    });
  });
}
