# HarmonyOS GeoDB Translations

## Overview
The HarmonyOS app now supports 7 languages, matching the Android version.

## Supported Languages

| Language | Locale Code (HarmonyOS) | Locale Code (Android) | App Name |
|----------|-------------------------|----------------------|----------|
| English (default) | `base` | `values` | GeoDB |
| German | `de` | `values-de` | GeoDB |
| Chinese (Simplified) | `zh_CN` | `values-zh-rCN` | 地理数据库 |
| French | `fr` | `values-fr` | GeoDB |
| Spanish | `es` | `values-es` | GeoDB |
| Italian | `it` | `values-it` | GeoDB |
| Portuguese | `pt` | `values-pt` | GeoDB |

## File Structure

### HarmonyOS
```
GeoDB/
├── AppScope/resources/
│   ├── base/element/string.json          # English (default)
│   ├── de/element/string.json            # German
│   ├── zh_CN/element/string.json         # Chinese
│   ├── fr/element/string.json            # French
│   ├── es/element/string.json            # Spanish
│   ├── it/element/string.json            # Italian
│   └── pt/element/string.json            # Portuguese
│
└── entry/src/main/resources/
    ├── base/element/string.json          # English (default)
    ├── de/element/string.json            # German
    ├── zh_CN/element/string.json         # Chinese
    ├── fr/element/string.json            # French
    ├── es/element/string.json            # Spanish
    ├── it/element/string.json            # Italian
    └── pt/element/string.json            # Portuguese
```

### Android (for comparison)
```
android-app/app/src/main/res/
├── values/strings.xml                    # English (default)
├── values-de/strings.xml                 # German
├── values-zh-rCN/strings.xml             # Chinese
├── values-fr/strings.xml                 # French
├── values-es/strings.xml                 # Spanish
├── values-it/strings.xml                 # Italian
└── values-pt/strings.xml                 # Portuguese
```

## Translation Keys

### AppScope (`AppScope/resources/{locale}/element/string.json`)
Contains app-level strings used in the launcher and system UI:

- `app_name` - App name shown in launcher

### Entry Module (`entry/src/main/resources/{locale}/element/string.json`)
Contains module-level strings used in the app description:

- `module_desc` - Module description
- `EntryAbility_desc` - Ability description
- `EntryAbility_label` - Ability label

## Sample Translations

### English (base)
```json
{
  "string": [
    {
      "name": "app_name",
      "value": "GeoDB"
    },
    {
      "name": "module_desc",
      "value": "Geographic database with cities, states, and countries"
    },
    {
      "name": "EntryAbility_desc",
      "value": "GeoDB - Fast geographic database search"
    }
  ]
}
```

### German (de)
```json
{
  "string": [
    {
      "name": "app_name",
      "value": "GeoDB"
    },
    {
      "name": "module_desc",
      "value": "Geografische Datenbank mit Städten, Bundesländern und Ländern"
    },
    {
      "name": "EntryAbility_desc",
      "value": "GeoDB - Schnelle geografische Datenbanksuche"
    }
  ]
}
```

### Chinese Simplified (zh_CN)
```json
{
  "string": [
    {
      "name": "app_name",
      "value": "地理数据库"
    },
    {
      "name": "module_desc",
      "value": "包含城市、州和国家的地理数据库"
    },
    {
      "name": "EntryAbility_desc",
      "value": "地理数据库 - 快速地理数据库搜索"
    }
  ]
}
```

## Locale Selection

HarmonyOS automatically selects the appropriate translation based on:
1. Device system language
2. Fallback to `base` (English) if language not available

### Testing Different Languages

**In DevEco Studio:**
1. Run on emulator/device
2. Settings → System → Language & Region
3. Select language to test
4. Relaunch app to see translations

**In Code (for testing):**
```typescript
// HarmonyOS automatically uses system locale
// Strings are referenced via $string:key_name
```

## Differences from Android

| Aspect | HarmonyOS | Android |
|--------|-----------|---------|
| **Format** | JSON | XML |
| **Locale codes** | `zh_CN` | `zh-rCN` |
| **Directory** | `resources/{locale}/element/` | `res/values-{locale}/` |
| **File name** | `string.json` | `strings.xml` |
| **Reference** | `$string:key_name` | `@string/key_name` |

## Adding New Languages

To add a new language (e.g., Japanese `ja`):

1. **Create directories:**
   ```bash
   mkdir -p GeoDB/AppScope/resources/ja/element
   mkdir -p GeoDB/entry/src/main/resources/ja/element
   ```

2. **Create `string.json` files:**
   - Copy from `base/element/string.json`
   - Translate values (keep keys in English)

3. **Test:**
   - Change device language to Japanese
   - Verify app shows Japanese strings

## Translation Sources

### Android Translations
All HarmonyOS translations are derived from the Android app:
- Location: `/GeoDB-Apps/android-app/app/src/main/res/values-{locale}/strings.xml`
- Maintained by: Same translation team
- Sync: Manual (copy from Android when updated)

### Contributing Translations
To improve or add translations:
1. Update Android XML files first
2. Convert to HarmonyOS JSON format
3. Test on both platforms
4. Submit PR with both versions

## Notes

- **App name:** "GeoDB" remains the same in most languages (except Chinese: "地理数据库")
- **UI strings:** Currently hardcoded in ArkTS code - consider extracting to resources
- **Format strings:** Not used yet (no `%s` or `%d` placeholders in HarmonyOS version)
- **RTL support:** Not yet implemented (would need for Arabic, Hebrew, etc.)

## Future Improvements

1. **Extract UI strings** from `Index.ets` to resource files:
   - Search mode labels ("Smart", "Cities", etc.)
   - Button labels ("Search", "Use Coordinates", etc.)
   - Status messages ("Loading database...", "No results found")

2. **Add more languages:**
   - Japanese (ja)
   - Korean (ko)
   - Russian (ru)
   - Arabic (ar) - requires RTL support

3. **Sync tool:** Create script to auto-convert Android XML to HarmonyOS JSON

## References

- [HarmonyOS Resource Management](https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/resource-categories-and-access-V5)
- [HarmonyOS Localization](https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/multi-languages-V5)
- [Android String Resources](https://developer.android.com/guide/topics/resources/string-resource)
