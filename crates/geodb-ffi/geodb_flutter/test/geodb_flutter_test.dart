import 'package:flutter_test/flutter_test.dart';
import 'package:geodb_flutter/geodb_flutter.dart';
import 'package:geodb_flutter/geodb_flutter_platform_interface.dart';
import 'package:geodb_flutter/geodb_flutter_method_channel.dart';
import 'package:geodb_flutter/models/models.dart';
import 'package:plugin_platform_interface/plugin_platform_interface.dart';

class MockGeodbFlutterPlatform
    with MockPlatformInterfaceMixin
    implements GeodbFlutterPlatform {

  @override
  Future<String?> getPlatformVersion() => Future.value('42');

  @override
  Future<void> initialize() => Future.value();

  @override
  Future<DbStats> getStats() => Future.value(DbStats(countries: 1, states: 1, cities: 1));

  @override
  Future<int> getCountryCount() => Future.value(1);

  @override
  Future<CityResult?> findCountryByCode(String code) => Future.value(null);

  @override
  Future<List<CityResult>> findCountriesBySubstring(String substring) => Future.value([]);

  @override
  Future<List<CityResult>> findStatesBySubstring(String substring) => Future.value([]);

  @override
  Future<List<CityResult>> findCitiesBySubstring(String substring) => Future.value([]);

  @override
  Future<List<CityResult>> findNearest({
    required double lat,
    required double lng,
    required int count,
  }) => Future.value([]);

  @override
  Future<List<CityResult>> findInRadius({
    required double lat,
    required double lng,
    required double radiusKm,
  }) => Future.value([]);

  @override
  Future<List<CityResult>> smartSearch(String query) => Future.value([]);
}

void main() {
  final GeodbFlutterPlatform initialPlatform = GeodbFlutterPlatform.instance;

  test('$MethodChannelGeodbFlutter is the default instance', () {
    expect(initialPlatform, isInstanceOf<MethodChannelGeodbFlutter>());
  });

  test('getPlatformVersion', () async {
    GeodbFlutter geodbFlutterPlugin = GeodbFlutter();
    MockGeodbFlutterPlatform fakePlatform = MockGeodbFlutterPlatform();
    GeodbFlutterPlatform.instance = fakePlatform;

    expect(await geodbFlutterPlugin.getPlatformVersion(), '42');
  });
}
