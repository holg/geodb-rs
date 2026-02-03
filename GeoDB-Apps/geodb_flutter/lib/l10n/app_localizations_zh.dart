// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Chinese (`zh`).
class AppLocalizationsZh extends AppLocalizations {
  AppLocalizationsZh([String locale = 'zh']) : super(locale);

  @override
  String get appTitle => '地理数据库 Flutter 示例';

  @override
  String get status => '状态';

  @override
  String get search => '搜索';

  @override
  String get searchHint => '输入城市、州或国家名称';

  @override
  String get smartSearch => '智能搜索';

  @override
  String get nearestToBerlin => '柏林附近';

  @override
  String get radiusSearch => '50公里范围';

  @override
  String get countries => '国家';

  @override
  String get noResults => '暂无结果。试试搜索！';

  @override
  String get loading => '加载中...';

  @override
  String get initializing => '正在初始化地理数据库...';

  @override
  String initSuccess(int countries, int states, int cities) {
    return '地理数据库初始化成功！\n国家：$countries，州：$states，城市：$cities';
  }

  @override
  String initFailed(String error) {
    return '初始化失败：$error';
  }

  @override
  String searchFailed(String error) {
    return '搜索失败：$error';
  }

  @override
  String foundNearest(int count) {
    return '找到柏林附近 $count 个城市';
  }

  @override
  String foundInRadius(int count) {
    return '在柏林50公里范围内找到 $count 个城市';
  }

  @override
  String foundCountries(int count) {
    return '找到 $count 个包含 United 的国家';
  }

  @override
  String failedFindNearest(String error) {
    return '查找附近城市失败：$error';
  }

  @override
  String failedFindRadius(String error) {
    return '查找范围内城市失败：$error';
  }

  @override
  String failedFindCountries(String error) {
    return '查找国家失败：$error';
  }
}
