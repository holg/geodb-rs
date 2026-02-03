// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for French (`fr`).
class AppLocalizationsFr extends AppLocalizations {
  AppLocalizationsFr([String locale = 'fr']) : super(locale);

  @override
  String get appTitle => 'Exemple GeoDB Flutter';

  @override
  String get status => 'Statut';

  @override
  String get search => 'Rechercher';

  @override
  String get searchHint => 'Entrez une ville, un état ou un pays';

  @override
  String get smartSearch => 'Recherche intelligente';

  @override
  String get nearestToBerlin => 'Près de Berlin';

  @override
  String get radiusSearch => 'Rayon de 50km';

  @override
  String get countries => 'Pays';

  @override
  String get noResults =>
      'Aucun résultat pour le moment. Essayez de rechercher!';

  @override
  String get loading => 'Chargement...';

  @override
  String get initializing => 'Initialisation de GeoDB...';

  @override
  String initSuccess(int countries, int states, int cities) {
    return 'GeoDB initialisé avec succès!\nPays: $countries, États: $states, Villes: $cities';
  }

  @override
  String initFailed(String error) {
    return 'Échec de l\'initialisation: $error';
  }

  @override
  String searchFailed(String error) {
    return 'Échec de la recherche: $error';
  }

  @override
  String foundNearest(int count) {
    return '$count villes les plus proches de Berlin trouvées';
  }

  @override
  String foundInRadius(int count) {
    return '$count villes trouvées dans un rayon de 50km autour de Berlin';
  }

  @override
  String foundCountries(int count) {
    return '$count pays contenant \"United\" trouvés';
  }

  @override
  String failedFindNearest(String error) {
    return 'Échec de la recherche des villes les plus proches: $error';
  }

  @override
  String failedFindRadius(String error) {
    return 'Échec de la recherche dans le rayon: $error';
  }

  @override
  String failedFindCountries(String error) {
    return 'Échec de la recherche de pays: $error';
  }
}
