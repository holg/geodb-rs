# Troubleshooting: "Loading database..." Stuck

## Problem

App shows "Loading database..." forever and never loads.

## Root Cause

**DevEco Studio Previewer does NOT support native libraries!**

The Previewer runs on your Mac (darwin) but our native library (`libgeodb_ffi.so`) is compiled for HarmonyOS (aarch64-linux-ohos). They're incompatible.

## Solution: Run on Real Device/Emulator

### Option 1: Use HarmonyOS Emulator

1. Open **Device Manager** in DevEco Studio
2. Create/Start a **HarmonyOS emulator** (not Previewer!)
3. Click **Run** → Select the emulator
4. Wait for app to install and launch

### Option 2: Use Physical Device

1. Enable **Developer Mode** on your HarmonyOS phone/tablet
2. Connect via USB
3. Click **Run** → Select your device

### Option 3: Build and Install HAP Manually

```bash
cd /path/to/GeoDB-Apps/harmonyos-cangjie/GeoDB

# Build HAP
./hvigorw assembleHap

# Find the HAP
find . -name "*.hap"

# Install on device
hdc install entry/build/default/outputs/default/entry-default-signed.hap

# Launch
hdc shell aa start -a EntryAbility -b com.trahe.geodb
```

## Verify Library is Present

Check that the native library exists:

```bash
ls -lh entry/libs/arm64-v8a/libgeodb_ffi.so
```

Expected output: **~9.4MB file** (contains 7.4MB embedded database)

```
-rwxr-xr-x  1 user  staff   9.4M Dec 10 12:26 libgeodb_ffi.so
```

## Check Build Logs

In DevEco Studio:

1. **View** → **Tool Windows** → **Build Output**
2. Look for CMake build messages
3. Check for errors building `geodb_napi`

Expected to see:
```
Building C++ object CMakeFiles/geodb_napi.dir/geodb_napi.cpp.o
Linking CXX shared library libgeodb_napi.so
```

## Common Issues

### Issue 1: Previewer Shows "Loading database..."

**Cause**: Previewer can't load native libraries

**Fix**: Use emulator or real device (see above)

### Issue 2: "Library not found" Error

**Cause**: Native library not packaged in HAP

**Fix**:
1. Check `entry/libs/arm64-v8a/libgeodb_ffi.so` exists
2. Rebuild: **Build** → **Rebuild Project**
3. Check build output for errors

### Issue 3: "dlopen failed" Error

**Cause**: Wrong architecture or missing dependencies

**Fix**:
```bash
# Verify library is ARM64 HarmonyOS
file entry/libs/arm64-v8a/libgeodb_ffi.so
# Should say: ELF 64-bit LSB shared object, ARM aarch64

# Check exported symbols
nm -D entry/libs/arm64-v8a/libgeodb_ffi.so | grep geodb_engine_new
# Should see: geodb_engine_new
```

### Issue 4: NAPI Bridge Not Building

**Cause**: CMake configuration issue

**Fix**:
1. Check `entry/src/main/cpp/CMakeLists.txt` exists
2. Check `entry/build-profile.json5` has `externalNativeOptions`
3. Clean and rebuild: **Build** → **Clean Project** → **Rebuild**

## Debug Using Logs

### View Logs via hdc

```bash
# Connect device
hdc shell

# Watch logs in real-time
hilog | grep -E "(GeoDB|geodb|FATAL)"
```

### Check for Specific Errors

```bash
# Check for library loading errors
hilog | grep "dlopen"

# Check for init errors
hilog | grep "geodb_engine_new"

# Check NAPI errors
hilog | grep "NAPI"
```

## Verify Database Loads

If the app **does** load eventually:

1. Check stats footer: Should show "Database: XXXXX cities"
2. Try searching: Type "Berlin" → should see results
3. Check logs: Should see "GeoDB initialized successfully"

## Expected Behavior

✅ **Correct**:
- Shows "Loading database..." for 1-2 seconds
- Then shows search interface
- Footer shows "Database: 148249 cities" (or similar)
- Search returns results instantly

❌ **Incorrect** (Previewer):
- Shows "Loading database..." forever
- No stats, no search interface
- No error messages

## Why This Happens

The GeoDB library embeds a **7.4MB compressed database** at compile time:

```rust
static EMBEDDED_DB: &[u8] = include_bytes!("../geodb_rs_data/geodb.flat.comp.blobs.bin");
```

This data is inside `libgeodb_ffi.so` (9.4MB total).

When `geodb_engine_new()` is called:
1. Decompresses GZIP data (~7.4MB → ~15MB)
2. Deserializes with bincode
3. Initializes spatial indexes

This takes 1-2 seconds on real hardware, but **fails silently in Previewer** because the library can't even load.

## Solution Summary

**DO NOT use DevEco Studio Previewer for native code apps!**

✅ Use: HarmonyOS Emulator
✅ Use: Real HarmonyOS device
❌ Don't use: Previewer (for native code)

---

**Next Step**: Start the HarmonyOS emulator in DevEco Studio and run the app there!
