// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for German (`de`).
class AppLocalizationsDe extends AppLocalizations {
  AppLocalizationsDe([String locale = 'de']) : super(locale);

  @override
  String get appTitle => 'GeoDB Flutter Beispiel';

  @override
  String get status => 'Status';

  @override
  String get search => 'Suchen';

  @override
  String get searchHint => 'Stadt, Bundesland oder Land eingeben';

  @override
  String get smartSearch => 'Intelligente Suche';

  @override
  String get nearestToBerlin => 'In der Nähe von Berlin';

  @override
  String get radiusSearch => '50km Umkreis';

  @override
  String get countries => 'Länder';

  @override
  String get noResults => 'Noch keine Ergebnisse. Versuchen Sie eine Suche!';

  @override
  String get loading => 'Lädt...';

  @override
  String get initializing => 'GeoDB wird initialisiert...';

  @override
  String initSuccess(int countries, int states, int cities) {
    return 'GeoDB erfolgreich initialisiert!\nLänder: $countries, Bundesländer: $states, Städte: $cities';
  }

  @override
  String initFailed(String error) {
    return 'Initialisierung fehlgeschlagen: $error';
  }

  @override
  String searchFailed(String error) {
    return 'Suche fehlgeschlagen: $error';
  }

  @override
  String foundNearest(int count) {
    return '$count nächstgelegene Städte zu Berlin gefunden';
  }

  @override
  String foundInRadius(int count) {
    return '$count Städte im Umkreis von 50km um Berlin gefunden';
  }

  @override
  String foundCountries(int count) {
    return '$count Länder mit \"United\" gefunden';
  }

  @override
  String failedFindNearest(String error) {
    return 'Suche nach nächstgelegenen Städten fehlgeschlagen: $error';
  }

  @override
  String failedFindRadius(String error) {
    return 'Suche im Umkreis fehlgeschlagen: $error';
  }

  @override
  String failedFindCountries(String error) {
    return 'Ländersuche fehlgeschlagen: $error';
  }
}
