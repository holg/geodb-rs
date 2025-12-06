# Android App Signing Setup

## Quick Start

### First Time Setup

```bash
cd GeoDB-Apps/android-app

# Run the setup script
./setup_signing.sh
```

This will:
1. ✅ Generate a release keystore
2. ✅ Create `.env` with signing credentials
3. ✅ Configure Gradle to use them

### Build Signed Release

```bash
# From repository root
./scripts/build_android_release.sh
```

This builds a signed AAB ready for Google Play Store!

## What Gets Created

### Files (gitignored - NEVER commit)

- `.env` - Contains keystore passwords
- `app/release.keystore` - Your signing key

### Example .env

```bash
KEYSTORE_FILE=release.keystore
KEYSTORE_PASSWORD=your-secret-password
KEY_ALIAS=geodb
KEY_PASSWORD=your-key-password
APPLICATION_ID=com.example.geodb
VERSION_NAME=0.1.4
VERSION_CODE=4
```

## Security Best Practices

### ✅ DO

- ✅ Backup your keystore to a safe location
- ✅ Store passwords in a password manager
- ✅ Keep `.env` and `.keystore` in `.gitignore`
- ✅ Use different passwords for keystore and key
- ✅ Set keystore validity to 10,000 days (default)

### ❌ DON'T

- ❌ Commit `.env` or `.keystore` to git
- ❌ Share keystore files publicly
- ❌ Use simple passwords
- ❌ Lose your keystore (can't update app!)

## Backup Your Keystore

**Critical:** Without the keystore, you cannot update your published app!

```bash
# Copy to safe location
cp app/release.keystore ~/Backup/geodb-keystore-$(date +%Y%m%d).keystore

# Save credentials to password manager
cat .env
```

## Google Play Upload Key vs App Signing Key

When you upload your first APK/AAB to Google Play:

1. **You sign with upload key** (the keystore we created)
2. **Google enrolls you in Play App Signing** (recommended)
3. **Google creates an app signing key** (stored by them)
4. **Google re-signs your app** before distribution

**Benefits:**
- If you lose upload key, Google can reset it
- App signing key is safe with Google
- Enhanced security

**Alternative (not recommended):**
- Opt out of Play App Signing
- Use your keystore as the only signing key
- If lost, app can NEVER be updated

## Troubleshooting

### "No .env file found"

Run setup:
```bash
./setup_signing.sh
```

### "Keystore not found"

Check path in `.env`:
```bash
cat .env | grep KEYSTORE_FILE
# Should be: KEYSTORE_FILE=release.keystore
```

Ensure keystore exists:
```bash
ls -la app/release.keystore
```

### "Wrong password"

Re-run setup:
```bash
./setup_signing.sh
# Choose "yes" to recreate
```

### "Unsigned APK produced"

Gradle didn't find `.env`. Check:
```bash
# .env should be in android-app directory
ls -la .env

# Run Gradle with verbose
cd GeoDB-Apps/android-app
./gradlew assembleRelease --info | grep signing
```

## Build Types

### Unsigned (for testing)

Delete `.env` temporarily:
```bash
mv .env .env.backup
./scripts/build_android_release.sh
mv .env.backup .env
```

### Signed AAB (for Play Store)

With `.env` present:
```bash
./scripts/build_android_release.sh
# Outputs: releases/android/app-release.aab (signed)
```

### Signed APK (for direct distribution)

```bash
cd GeoDB-Apps/android-app
./gradlew assembleRelease
# Outputs: app/build/outputs/apk/release/app-release.apk (signed)
```

## Verify Signature

Check if APK/AAB is signed:

```bash
# For APK
jarsigner -verify -verbose releases/android/app-release.apk

# For AAB
jarsigner -verify -verbose releases/android/app-release.aab
```

## Update App Version

Edit `.env`:
```bash
VERSION_NAME=0.1.5
VERSION_CODE=5
```

Or update `app/build.gradle.kts` directly.

## CI/CD Integration

For GitHub Actions, use secrets:

```yaml
- name: Build signed release
  env:
    KEYSTORE_PASSWORD: ${{ secrets.KEYSTORE_PASSWORD }}
    KEY_PASSWORD: ${{ secrets.KEY_PASSWORD }}
    KEYSTORE_FILE: ${{ secrets.KEYSTORE_BASE64 }}
  run: |
    echo "$KEYSTORE_FILE" | base64 -d > app/release.keystore
    ./scripts/build_android_release.sh
```

Store in GitHub Secrets:
- `KEYSTORE_PASSWORD`
- `KEY_PASSWORD`
- `KEYSTORE_BASE64` (base64-encoded keystore)

## References

- [Android App Signing](https://developer.android.com/studio/publish/app-signing)
- [Google Play App Signing](https://support.google.com/googleplay/android-developer/answer/9842756)
