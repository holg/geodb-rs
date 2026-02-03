// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Spanish Castilian (`es`).
class AppLocalizationsEs extends AppLocalizations {
  AppLocalizationsEs([String locale = 'es']) : super(locale);

  @override
  String get appTitle => 'Ejemplo de GeoDB Flutter';

  @override
  String get status => 'Estado';

  @override
  String get search => 'Buscar';

  @override
  String get searchHint => 'Ingrese ciudad, estado o país';

  @override
  String get smartSearch => 'Búsqueda inteligente';

  @override
  String get nearestToBerlin => 'Cerca de Berlín';

  @override
  String get radiusSearch => 'Radio de 50km';

  @override
  String get countries => 'Países';

  @override
  String get noResults => 'Sin resultados aún. ¡Intenta buscar!';

  @override
  String get loading => 'Cargando...';

  @override
  String get initializing => 'Iniciando GeoDB...';

  @override
  String initSuccess(int countries, int states, int cities) {
    return '¡GeoDB iniciado correctamente!\nPaíses: $countries, Estados: $states, Ciudades: $cities';
  }

  @override
  String initFailed(String error) {
    return 'Error al inicializar: $error';
  }

  @override
  String searchFailed(String error) {
    return 'Búsqueda fallida: $error';
  }

  @override
  String foundNearest(int count) {
    return 'Se encontraron $count ciudades cercanas a Berlín';
  }

  @override
  String foundInRadius(int count) {
    return 'Se encontraron $count ciudades en un radio de 50km de Berlín';
  }

  @override
  String foundCountries(int count) {
    return 'Se encontraron $count países con \"United\"';
  }

  @override
  String failedFindNearest(String error) {
    return 'Error al buscar ciudades cercanas: $error';
  }

  @override
  String failedFindRadius(String error) {
    return 'Error al buscar en radio: $error';
  }

  @override
  String failedFindCountries(String error) {
    return 'Error al buscar países: $error';
  }
}
