import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'geodb_flutter_platform_interface.dart';
import 'models/models.dart';

/// An implementation of [GeodbFlutterPlatform] that uses method channels.
class MethodChannelGeodbFlutter extends GeodbFlutterPlatform {
  /// The method channel used to interact with the native platform.
  @visibleForTesting
  final methodChannel = const MethodChannel('geodb_flutter');

  @override
  Future<void> initialize() async {
    await methodChannel.invokeMethod<void>('initialize');
  }

  @override
  Future<DbStats> getStats() async {
    final result = await methodChannel.invokeMethod<Map>('getStats');
    return DbStats.fromMap(Map<String, dynamic>.from(result!));
  }

  @override
  Future<int> getCountryCount() async {
    final result = await methodChannel.invokeMethod<int>('getCountryCount');
    return result!;
  }

  @override
  Future<CityResult?> findCountryByCode(String code) async {
    final result = await methodChannel.invokeMethod<Map>('findCountryByCode', {
      'code': code,
    });
    return result != null
        ? CityResult.fromMap(Map<String, dynamic>.from(result))
        : null;
  }

  @override
  Future<List<CityResult>> findCountriesBySubstring(String substring) async {
    final result = await methodChannel.invokeMethod<List>('findCountriesBySubstring', {
      'substring': substring,
    });
    return (result ?? [])
        .map((e) => CityResult.fromMap(Map<String, dynamic>.from(e as Map)))
        .toList();
  }

  @override
  Future<List<CityResult>> findStatesBySubstring(String substring) async {
    final result = await methodChannel.invokeMethod<List>('findStatesBySubstring', {
      'substring': substring,
    });
    return (result ?? [])
        .map((e) => CityResult.fromMap(Map<String, dynamic>.from(e as Map)))
        .toList();
  }

  @override
  Future<List<CityResult>> findCitiesBySubstring(String substring) async {
    final result = await methodChannel.invokeMethod<List>('findCitiesBySubstring', {
      'substring': substring,
    });
    return (result ?? [])
        .map((e) => CityResult.fromMap(Map<String, dynamic>.from(e as Map)))
        .toList();
  }

  @override
  Future<List<CityResult>> findNearest({
    required double lat,
    required double lng,
    required int count,
  }) async {
    final result = await methodChannel.invokeMethod<List>('findNearest', {
      'lat': lat,
      'lng': lng,
      'count': count,
    });
    return (result ?? [])
        .map((e) => CityResult.fromMap(Map<String, dynamic>.from(e as Map)))
        .toList();
  }

  @override
  Future<List<CityResult>> findInRadius({
    required double lat,
    required double lng,
    required double radiusKm,
  }) async {
    final result = await methodChannel.invokeMethod<List>('findInRadius', {
      'lat': lat,
      'lng': lng,
      'radiusKm': radiusKm,
    });
    return (result ?? [])
        .map((e) => CityResult.fromMap(Map<String, dynamic>.from(e as Map)))
        .toList();
  }

  @override
  Future<List<CityResult>> smartSearch(String query) async {
    final result = await methodChannel.invokeMethod<List>('smartSearch', {
      'query': query,
    });
    return (result ?? [])
        .map((e) => CityResult.fromMap(Map<String, dynamic>.from(e as Map)))
        .toList();
  }

  @override
  Future<String?> getPlatformVersion() async {
    final version = await methodChannel.invokeMethod<String>('getPlatformVersion');
    return version;
  }
}
