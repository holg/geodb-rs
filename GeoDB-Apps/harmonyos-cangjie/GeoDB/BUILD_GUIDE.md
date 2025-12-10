# GeoDB HarmonyOS Build Guide

## What We Built

A hybrid HarmonyOS app using:
- **ArkTS** for UI (DevEco Studio's primary UI framework)
- **NAPI C++** bridge for native integration
- **Rust** for the database engine (via C FFI)

## Architecture

```
┌─────────────────────────────────────────────┐
│          ArkTS UI (Index.ets)               │
│         GeoDatabase.ets wrapper             │
├─────────────────────────────────────────────┤
│    NAPI Bridge (geodb_napi.cpp)            │
├─────────────────────────────────────────────┤
│    Rust C API (libgeodb_ffi.so)            │
│         geodb-core engine                   │
└─────────────────────────────────────────────┘
```

## Prerequisites

1. ✅ DevEco Studio installed
2. ✅ HarmonyOS NEXT SDK (API 12+)
3. ✅ Rust toolchain with HarmonyOS target

## Step 1: Build Rust Library

### Install HarmonyOS Rust target

```bash
rustup target add aarch64-unknown-linux-ohos
```

### Build the library

```bash
cd /path/to/geodb-rs/crates/geodb-ffi

# Build for HarmonyOS
cargo build --release --target aarch64-unknown-linux-ohos

# Create libs directory in DevEco project
mkdir -p /path/to/GeoDB-Apps/harmonyos-cangjie/GeoDB/entry/libs/arm64-v8a

# Copy the library
cp ../../../target/aarch64-unknown-linux-ohos/release/libgeodb_ffi.so \
   /path/to/GeoDB-Apps/harmonyos-cangjie/GeoDB/entry/libs/arm64-v8a/
```

## Step 2: Open Project in DevEco Studio

1. Launch DevEco Studio
2. Open the `GeoDB` project: `/path/to/GeoDB-Apps/harmonyos-cangjie/GeoDB`
3. Wait for sync to complete

## Step 3: Build the Project

### Option A: Via DevEco Studio UI

1. Click **Build** → **Build Hap(s)/App(s)**
2. Wait for compilation to complete
3. Check for errors in the build output

### Option B: Via Command Line

```bash
cd /path/to/GeoDB-Apps/harmonyos-cangjie/GeoDB

# Clean build
./hvigorw clean

# Build
./hvigorw assembleHap
```

## Step 4: Run on Device/Emulator

### Using Emulator

1. Start HarmonyOS emulator from DevEco Studio
2. Click **Run** button
3. Select the emulator

### Using Physical Device

1. Enable Developer Mode on HarmonyOS device
2. Connect via USB
3. Click **Run** → Select your device

Or via command line:

```bash
# Install HAP
hdc install entry/build/default/outputs/default/entry-default-signed.hap

# Launch app
hdc shell aa start -a EntryAbility -b com.trahe.geodb
```

## Project Structure

```
GeoDB/
├── entry/
│   ├── src/main/
│   │   ├── ets/                    # ArkTS UI code
│   │   │   ├── pages/
│   │   │   │   └── Index.ets      # Main UI
│   │   │   └── model/
│   │   │       └── GeoDatabase.ets # Native wrapper
│   │   ├── cpp/                    # NAPI bridge
│   │   │   ├── CMakeLists.txt
│   │   │   └── geodb_napi.cpp
│   │   ├── cangjie/                # Pure Cangjie code (future use)
│   │   │   ├── geodb/
│   │   │   └── ui/
│   │   └── resources/
│   ├── libs/                       # Native libraries
│   │   └── arm64-v8a/
│   │       └── libgeodb_ffi.so
│   └── build-profile.json5         # Build configuration
├── build-profile.json5
└── oh-package.json5
```

## Troubleshooting

### Error: Library not found

**Problem**: `dlopen failed: library "libgeodb_ffi.so" not found`

**Solution**:
1. Check that `libgeodb_ffi.so` exists in `entry/libs/arm64-v8a/`
2. Verify `build-profile.json5` has correct `abiFilters`
3. Clean and rebuild: `./hvigorw clean && ./hvigorw assembleHap`

### Error: Symbol not found

**Problem**: `dlsym failed: symbol "geodb_engine_new" not found`

**Solution**:
1. Verify Rust functions are `#[no_mangle]` and `extern "C"`
2. Check symbols: `nm -D libgeodb_ffi.so | grep geodb`
3. Rebuild Rust library

### Error: NAPI module not loaded

**Problem**: `Cannot find module 'libgeodb_napi.so'`

**Solution**:
1. Check CMakeLists.txt is correct
2. Verify `externalNativeOptions` in build-profile.json5
3. Clean DevEco cache: **Build** → **Clean Project**

### Error: Compilation errors in C++

**Problem**: Various C++ compilation errors

**Solution**:
1. Check HarmonyOS NDK is installed
2. Verify NAPI headers are available
3. Update DevEco Studio to latest version

## Testing

### Basic Tests

1. **App launches** - No crash on startup
2. **Database loads** - "Loading database..." appears briefly
3. **Search works** - Type "Berlin" and see results
4. **Results display** - City names, flags, population shown
5. **Details work** - Tap city to see dialog

### Performance Tests

1. **Initial load time** - Should be < 2 seconds
2. **Search response** - Should be instant (<100ms)
3. **Scroll smoothness** - 60 FPS scrolling
4. **Memory usage** - Check with DevEco Profiler

## What's Included

✅ **NAPI C++ bridge** - Connects ArkTS to Rust
✅ **ArkTS UI** - Complete search interface with results
✅ **Native integration** - CMake and build configuration
✅ **Cangjie code** - Ready for future pure-Cangjie version
✅ **Error handling** - Graceful failures with logs

## What's NOT Included (Yet)

⬜ **Pure Cangjie UI** - Currently using ArkTS (Cangjie plugin support limited)
⬜ **Async operations** - Search is sync (good enough for now)
⬜ **More search modes** - Only smart search implemented
⬜ **Map integration** - No map view yet

## Next Steps

1. **Build and test** the app
2. **Verify native library** loads correctly
3. **Test search** functionality
4. **Profile performance** if needed
5. **Add more features** (map, favorites, etc.)

## Migration to Pure Cangjie (Future)

When Cangjie plugin matures:

1. The `cangjie/` directory already has the backend code
2. Remove NAPI bridge (geodb_napi.cpp)
3. Call Cangjie directly from ArkUI
4. Or rewrite UI in pure Cangjie

The hard work (FFI layer) is done! 🎉

## Support

- **DevEco Studio**: https://developer.huawei.com/consumer/en/deveco-studio/
- **HarmonyOS Docs**: https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/
- **NAPI Guide**: https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/napi-guidelines-V5

---

**Status**: Ready to build! 🚀

All code is in place. Just build the Rust library and run in DevEco Studio.
