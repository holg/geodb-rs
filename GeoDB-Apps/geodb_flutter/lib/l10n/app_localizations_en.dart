// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'GeoDB Flutter Example';

  @override
  String get status => 'Status';

  @override
  String get search => 'Search';

  @override
  String get searchHint => 'Enter city, state, or country name';

  @override
  String get smartSearch => 'Smart Search';

  @override
  String get nearestToBerlin => 'Nearest to Berlin';

  @override
  String get radiusSearch => '50km Radius';

  @override
  String get countries => 'Countries';

  @override
  String get noResults => 'No results yet. Try searching!';

  @override
  String get loading => 'Loading...';

  @override
  String get initializing => 'Initializing GeoDB...';

  @override
  String initSuccess(int countries, int states, int cities) {
    return 'GeoDB initialized successfully!\nCountries: $countries, States: $states, Cities: $cities';
  }

  @override
  String initFailed(String error) {
    return 'Failed to initialize: $error';
  }

  @override
  String searchFailed(String error) {
    return 'Search failed: $error';
  }

  @override
  String foundNearest(int count) {
    return 'Found $count nearest cities to Berlin';
  }

  @override
  String foundInRadius(int count) {
    return 'Found $count cities within 50km of Berlin';
  }

  @override
  String foundCountries(int count) {
    return 'Found $count countries with \"United\"';
  }

  @override
  String failedFindNearest(String error) {
    return 'Failed to find nearest: $error';
  }

  @override
  String failedFindRadius(String error) {
    return 'Failed to find in radius: $error';
  }

  @override
  String failedFindCountries(String error) {
    return 'Failed to find countries: $error';
  }
}
