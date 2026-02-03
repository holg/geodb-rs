// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Italian (`it`).
class AppLocalizationsIt extends AppLocalizations {
  AppLocalizationsIt([String locale = 'it']) : super(locale);

  @override
  String get appTitle => 'Esempio GeoDB Flutter';

  @override
  String get status => 'Stato';

  @override
  String get search => 'Cerca';

  @override
  String get searchHint => 'Inserisci città, stato o paese';

  @override
  String get smartSearch => 'Ricerca intelligente';

  @override
  String get nearestToBerlin => 'Vicino a Berlino';

  @override
  String get radiusSearch => 'Raggio di 50km';

  @override
  String get countries => 'Paesi';

  @override
  String get noResults => 'Nessun risultato ancora. Prova a cercare!';

  @override
  String get loading => 'Caricamento...';

  @override
  String get initializing => 'Inizializzazione di GeoDB...';

  @override
  String initSuccess(int countries, int states, int cities) {
    return 'GeoDB inizializzato con successo!\nPaesi: $countries, Stati: $states, Città: $cities';
  }

  @override
  String initFailed(String error) {
    return 'Inizializzazione fallita: $error';
  }

  @override
  String searchFailed(String error) {
    return 'Ricerca fallita: $error';
  }

  @override
  String foundNearest(int count) {
    return 'Trovate $count città più vicine a Berlino';
  }

  @override
  String foundInRadius(int count) {
    return 'Trovate $count città nel raggio di 50km da Berlino';
  }

  @override
  String foundCountries(int count) {
    return 'Trovati $count paesi con \"United\"';
  }

  @override
  String failedFindNearest(String error) {
    return 'Ricerca città vicine fallita: $error';
  }

  @override
  String failedFindRadius(String error) {
    return 'Ricerca nel raggio fallita: $error';
  }

  @override
  String failedFindCountries(String error) {
    return 'Ricerca paesi fallita: $error';
  }
}
