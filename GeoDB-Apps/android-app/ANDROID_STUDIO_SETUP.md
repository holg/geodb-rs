# Loading GeoDB Android App in Android Studio

## Prerequisites

1. **Download Android Studio**: https://developer.android.com/studio
2. **Install it**: Follow platform-specific installer

## Step-by-Step: Open Project

### 1. Launch Android Studio

Open the Android Studio application

### 2. Open the Project

**Option A: From Welcome Screen**
- Click **"Open"**
- Navigate to: `/Users/htr/Documents/develeop/rust/geodb-rs/GeoDB-Apps/android-app`
- Select the **`android-app`** folder
- Click **"Open"**

**Option B: From Menu Bar**
- File → Open
- Navigate to: `/Users/htr/Documents/develeop/rust/geodb-rs/GeoDB-Apps/android-app`
- Click **"Open"**

### 3. Trust the Project

Android Studio will ask:
- **"Trust and Open Project 'android-app'?"**
- Click **"Trust Project"**

### 4. Wait for Gradle Sync

You'll see in the bottom status bar:
```
Gradle sync in progress...
```

This takes **2-10 minutes** on first open because it:
- Downloads Gradle wrapper
- Downloads dependencies
- Indexes project files
- Configures build tools

**Be patient!** Don't interrupt this process.

### 5. Check for Errors

After sync completes, check the **Build** panel (bottom):

**✅ Success:**
```
BUILD SUCCESSFUL in 3m 42s
```

**❌ Common Errors:**

#### Error: "SDK location not found"

**Fix:**
1. Tools → SDK Manager
2. Install Android SDK Platform 34 (or latest)
3. Click Apply

Or create `local.properties`:
```bash
cd GeoDB-Apps/android-app
echo "sdk.dir=$HOME/Library/Android/sdk" > local.properties
```

#### Error: "NDK not configured"

**Fix:**
1. Tools → SDK Manager
2. SDK Tools tab
3. Check: ☑ NDK (Side by side)
4. Check: ☑ CMake
5. Click Apply

#### Error: "Gradle version incompatible"

**Fix:** Android Studio will show a banner:
- Click **"Upgrade Gradle version"**
- Or: File → Project Structure → Project → Gradle Version

### 6. Build the Project

Once Gradle sync succeeds:

1. **Build → Make Project** (or ⌘F9 / Ctrl+F9)
2. Wait for build to complete
3. Check **Build** panel for success

### 7. Run on Emulator or Device

**Create an Emulator:**
1. Tools → Device Manager
2. Click **"Create Device"**
3. Select **Phone → Pixel 7**
4. Download a system image (e.g., API 34)
5. Click **"Finish"**

**Run the app:**
1. Click green **▶️ Run** button (or Shift+F10)
2. Select emulator or connected device
3. Click **"OK"**

App should launch!

## Project Structure

Once loaded, you'll see:

```
android-app/
├── app/
│   ├── src/
│   │   ├── main/
│   │   │   ├── java/com/example/geodb/  # Kotlin code
│   │   │   ├── res/                      # Resources (layouts, icons)
│   │   │   ├── AndroidManifest.xml       # App config
│   │   │   └── jniLibs/                  # Native libraries (built by Rust)
│   │   └── test/                         # Tests
│   └── build.gradle.kts                  # App build config
├── build.gradle.kts                      # Project build config
├── settings.gradle.kts                   # Gradle settings
├── .env.example                          # Template for signing config
└── setup_signing.sh                      # Setup script

```

## Before Building Release

The project needs native Rust libraries. Two options:

### Option 1: Auto-build (via script)

```bash
# From repository root
./scripts/build_android_release.sh
```

This builds Rust libraries AND Android APK/AAB.

### Option 2: Manual build in Android Studio

1. First build Rust libraries:
```bash
cd crates/geodb-ffi
cargo ndk --target aarch64-linux-android build --release
cargo ndk --target armv7-linux-androideabi build --release
cargo ndk --target x86_64-linux-android build --release
cargo ndk --target i686-linux-android build --release
```

2. Copy to Android app:
```bash
./scripts/build_android_release.sh
```

3. Then build in Android Studio:
   - Build → Generate Signed Bundle/APK

## Setup Signing (for Release)

Before building release APK/AAB:

```bash
cd GeoDB-Apps/android-app
./setup_signing.sh
```

This creates:
- `.env` - Your signing credentials
- `app/release.keystore` - Your signing key

See `SIGNING_README.md` for details.

## Troubleshooting

### "Could not find method implementation()"

**Cause:** Gradle version mismatch

**Fix:**
1. Check `gradle/wrapper/gradle-wrapper.properties`
2. Ensure: `distributionUrl=https\://services.gradle.org/distributions/gradle-8.0-bin.zip`
3. Sync Gradle

### "Failed to resolve: androidx.core:core-ktx"

**Cause:** Repositories not configured

**Fix:**
1. Check internet connection
2. Tools → SDK Manager → SDK Update Sites
3. Force refresh: File → Invalidate Caches → Invalidate and Restart

### "No connected devices"

**Cause:** No emulator or physical device

**Fix:**
1. Tools → Device Manager
2. Create virtual device
3. Or connect physical device via USB with USB debugging enabled

### "Build failed: jniLibs not found"

**Cause:** Rust libraries not built yet

**Fix:**
```bash
# Build Rust libraries first
./scripts/build_android_release.sh
```

Or comment out the native library loading temporarily to test UI.

## Key Files to Know

### `app/build.gradle.kts`
- App configuration
- Dependencies
- Build types (debug/release)
- Signing configuration

### `app/src/main/AndroidManifest.xml`
- App permissions
- Activities (screens)
- App icon and name

### `app/src/main/java/com/example/geodb/MainActivity.kt`
- Main app code
- Where native library is loaded

## Useful Shortcuts

- **Build Project**: ⌘F9 (Mac) / Ctrl+F9 (Windows/Linux)
- **Run App**: Shift+F10
- **Clean Project**: Build → Clean Project
- **Rebuild**: Build → Rebuild Project
- **Sync Gradle**: File → Sync Project with Gradle Files

## Next Steps

Once loaded successfully:

1. ✅ Explore the code in `app/src/main/java/`
2. ✅ Modify layouts in `app/src/main/res/layout/`
3. ✅ Test on emulator
4. ✅ Build release with `./scripts/build_android_release.sh`
5. ✅ Upload to Google Play Console

## Need Help?

- Android Studio docs: https://developer.android.com/studio/intro
- Gradle docs: https://docs.gradle.org
- Check `SIGNING_README.md` for release builds
