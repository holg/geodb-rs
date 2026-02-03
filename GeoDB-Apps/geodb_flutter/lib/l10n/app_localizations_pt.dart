// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Portuguese (`pt`).
class AppLocalizationsPt extends AppLocalizations {
  AppLocalizationsPt([String locale = 'pt']) : super(locale);

  @override
  String get appTitle => 'Exemplo GeoDB Flutter';

  @override
  String get status => 'Status';

  @override
  String get search => 'Pesquisar';

  @override
  String get searchHint => 'Digite cidade, estado ou país';

  @override
  String get smartSearch => 'Pesquisa inteligente';

  @override
  String get nearestToBerlin => 'Perto de Berlim';

  @override
  String get radiusSearch => 'Raio de 50km';

  @override
  String get countries => 'Países';

  @override
  String get noResults => 'Sem resultados ainda. Tente pesquisar!';

  @override
  String get loading => 'Carregando...';

  @override
  String get initializing => 'Inicializando GeoDB...';

  @override
  String initSuccess(int countries, int states, int cities) {
    return 'GeoDB inicializado com sucesso!\nPaíses: $countries, Estados: $states, Cidades: $cities';
  }

  @override
  String initFailed(String error) {
    return 'Falha na inicialização: $error';
  }

  @override
  String searchFailed(String error) {
    return 'Falha na pesquisa: $error';
  }

  @override
  String foundNearest(int count) {
    return 'Encontradas $count cidades mais próximas de Berlim';
  }

  @override
  String foundInRadius(int count) {
    return 'Encontradas $count cidades num raio de 50km de Berlim';
  }

  @override
  String foundCountries(int count) {
    return 'Encontrados $count países com \"United\"';
  }

  @override
  String failedFindNearest(String error) {
    return 'Falha ao procurar cidades próximas: $error';
  }

  @override
  String failedFindRadius(String error) {
    return 'Falha ao procurar no raio: $error';
  }

  @override
  String failedFindCountries(String error) {
    return 'Falha ao procurar países: $error';
  }
}
