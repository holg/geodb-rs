# DevEco Studio Integration Guide

This guide shows how to integrate the GeoDB Cangjie code into a DevEco Studio HarmonyOS NEXT project.

## Prerequisites

- ✅ DevEco Studio 5.0.3+ installed
- ✅ Cangjie plugin installed
- ✅ HarmonyOS NEXT SDK (API 12+)
- ✅ Rust toolchain with HarmonyOS targets

## Step 1: Create HarmonyOS Project

1. Open DevEco Studio
2. File → New → Create Project
3. Select **Application** → **Empty Ability**
4. Configuration:
   - Project name: `GeoDB`
   - Bundle name: `com.trahe.geodb`
   - Language: **Cangjie** (not ArkTS)
   - API: 12+
   - Device type: Phone, Tablet, Watch, etc.

## Step 2: Build Rust Native Library

### For HarmonyOS Targets

```bash
cd /path/to/geodb-rs/crates/geodb-ffi

# Install HarmonyOS Rust targets if not already done
rustup target add aarch64-unknown-linux-ohos
rustup target add armv7-unknown-linux-ohos

# Build for arm64 (primary target)
cargo build --release --target aarch64-unknown-linux-ohos

# Copy to DevEco project
mkdir -p /path/to/GeoDB/entry/libs/arm64-v8a
cp ../../../target/aarch64-unknown-linux-ohos/release/libgeodb_ffi.so \
   /path/to/GeoDB/entry/libs/arm64-v8a/
```

### Alternative: Use cargo-ndk (if targeting older Android compat mode)

```bash
cargo install cargo-ndk

# Build for multiple ABIs
cargo ndk -t arm64-v8a -t armeabi-v7a \
  -o /path/to/GeoDB/entry/libs \
  build --release
```

## Step 3: Copy Cangjie Source Files

```bash
# From this directory
cd GeoDB-Apps/harmonyos-cangjie

# Copy source files to DevEco project
cp -r src/* /path/to/GeoDB/entry/src/main/cangjie/
```

Your DevEco project should now have:

```
GeoDB/entry/src/main/cangjie/
├── geodb/
│   ├── ffi.cj
│   ├── types.cj
│   └── engine.cj
├── ui/
│   └── ContentView.cj
└── main.cj
```

## Step 4: Configure Native Library Loading

### Edit `entry/build-profile.json5`

Add native library configuration:

```json5
{
  "apiType": "stageMode",
  "buildOption": {
    "arkOptions": {
      "runtimeOnly": {
        "sources": [
          "./src/main/cangjie"
        ]
      }
    }
  },
  "targets": [
    {
      "name": "default",
      "runtimeOS": "HarmonyOS",
      "compatibleSdkVersion": "5.0.0(12)"
    }
  ],
  // Add this section for native libraries
  "externalNativeOptions": {
    "path": "./src/main/cpp/CMakeLists.txt",
    "arguments": "",
    "cppFlags": "",
    "abiFilters": ["arm64-v8a"]
  }
}
```

### Create CMakeLists.txt (if not exists)

Create `entry/src/main/cpp/CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.5.0)
project(geodb_native)

# Add prebuilt library
add_library(geodb_ffi SHARED IMPORTED)
set_target_properties(geodb_ffi PROPERTIES
    IMPORTED_LOCATION ${CMAKE_CURRENT_SOURCE_DIR}/../../../libs/${ANDROID_ABI}/libgeodb_ffi.so
)

# Link to main module (if you have additional native code)
# Otherwise, the library will be packaged automatically
```

## Step 5: Integrate with ArkUI

Since we have business logic in `ContentView.cj`, we need to create the ArkUI UI layer.

### Option A: Pure ArkUI (Recommended)

Create `entry/src/main/ets/pages/Index.ets`:

```typescript
// This is ArkTS - HarmonyOS's UI language
// It will call our Cangjie backend

import { ContentView } from '../cangjie/ui/ContentView'

@Entry
@Component
struct Index {
  @State viewModel: ContentView = new ContentView()
  @State searchText: string = ''

  build() {
    Navigation() {
      Column() {
        if (!this.viewModel.isInitialized) {
          // Loading state
          Row() {
            LoadingProgress()
            Text('Loading database...')
              .fontSize(14)
              .margin({ left: 8 })
          }
          .justifyContent(FlexAlign.Center)
          .padding(20)
        } else {
          // Search interface
          Column() {
            // Search bar
            TextInput({ placeholder: 'Search city...' })
              .onChange((value: string) => {
                this.searchText = value
              })
              .onSubmit(() => {
                this.viewModel.onSearchTextChanged(this.searchText)
                this.viewModel.performSearch()
              })
              .margin({ top: 10, left: 16, right: 16 })

            // Results list
            List({ space: 8 }) {
              ForEach(this.viewModel.getDisplayResults(), (city: CityResult) => {
                ListItem() {
                  Row() {
                    Text(this.viewModel.countryFlag(city.iso2))
                      .fontSize(24)
                      .margin({ right: 12 })

                    Column({ space: 4 }) {
                      Text(city.name)
                        .fontSize(16)
                        .fontWeight(FontWeight.Medium)

                      Text(`${city.state ? city.state + ', ' : ''}${city.country}`)
                        .fontSize(12)
                        .fontColor(Color.Gray)
                    }
                    .alignItems(HorizontalAlign.Start)
                  }
                  .width('100%')
                  .padding(12)
                }
                .onClick(() => {
                  // Navigate to detail page
                  // router.pushUrl({ url: 'pages/Detail', params: { city: city } })
                })
              })
            }
            .layoutWeight(1)
            .margin({ top: 16 })

            // Stats footer
            if (this.viewModel.getStatsText()) {
              Text(this.viewModel.getStatsText())
                .fontSize(12)
                .fontColor(Color.Gray)
                .margin(12)
            }
          }
        }
      }
      .width('100%')
      .height('100%')
    }
    .title('GeoDB')
    .titleMode(NavigationTitleMode.Mini)
  }
}
```

### Option B: Pure Cangjie with Custom Rendering (Advanced)

If you want pure Cangjie without ArkTS:

1. Use Cangjie's ArkUI bindings (check Cangjie docs for latest API)
2. Write UI declaratively in Cangjie
3. This requires Cangjie ArkUI wrapper library

**Note**: As of early 2025, ArkUI bindings for Cangjie may be limited. ArkTS is the primary UI language.

## Step 6: Module Dependencies

### Edit `entry/oh-package.json5`

```json5
{
  "name": "entry",
  "version": "1.0.0",
  "description": "GeoDB HarmonyOS App",
  "main": "index.ets",
  "author": "",
  "license": "MIT",
  "dependencies": {
    // Add any required HarmonyOS modules
    "@ohos/hypium": "1.0.0"
  }
}
```

## Step 7: Build and Run

### Build

1. In DevEco Studio, click **Build** → **Build Hap(s)/App(s)**
2. Or use command line:
   ```bash
   cd /path/to/GeoDB
   hvigorw assembleHap
   ```

### Run

1. Connect HarmonyOS device or start emulator
2. Click **Run** button in DevEco Studio
3. Or command line:
   ```bash
   hdc install entry/build/default/outputs/default/entry-default-signed.hap
   ```

## Step 8: Testing

### Test Checklist

- [ ] App launches without crash
- [ ] Database initializes (check "Loading database..." appears briefly)
- [ ] Search bar is responsive
- [ ] Search returns results
- [ ] Results display correctly with flags
- [ ] Scrolling is smooth
- [ ] Memory doesn't leak (check with DevEco Profiler)

### Debug Native Code

If FFI crashes:

1. Enable native debugging in DevEco Studio
2. Add breakpoints in Rust code
3. Check lldb console for crashes
4. Use `hdc shell hilog` to see system logs

```bash
# Watch logs
hdc shell hilog -t geodb_ffi
```

## Common Issues

### Issue 1: Library Not Found

**Error**: `java.lang.UnsatisfiedLinkError: dlopen failed: library "libgeodb_ffi.so" not found`

**Fix**:
- Check library is in `entry/libs/arm64-v8a/`
- Verify ABIFilters in build-profile.json5
- Rebuild: `hvigorw clean && hvigorw assembleHap`

### Issue 2: Symbol Not Found

**Error**: `java.lang.UnsatisfiedLinkError: dlsym failed: symbol "geodb_engine_new" not found`

**Fix**:
- Ensure Rust functions are `#[no_mangle]`
- Check with: `nm -D libgeodb_ffi.so | grep geodb_engine_new`
- Verify `extern "C"` in c_api.rs

### Issue 3: Cangjie Compilation Errors

**Error**: Various Cangjie syntax errors

**Fix**:
- Check Cangjie version matches code (written for Cangjie 1.0)
- Update string conversion helpers to use correct std lib
- Consult: https://docs.cangjie-lang.cn/

### Issue 4: Memory Crashes

**Error**: App crashes during search

**Fix**:
- Add more defensive null checks in FFI layer
- Ensure all C strings are freed
- Test with smaller result sets first
- Use DevEco Studio Memory Profiler

## Performance Optimization

1. **Lazy Loading**: Database loads on first query (already implemented)
2. **Result Caching**: Cache search results in ContentView
3. **Background Threading**: Move search to background thread
4. **Release Build**: Always test performance in Release mode

## Distribution

### App Signing

1. Generate signing certificate in DevEco Studio
2. Configure in **File** → **Project Structure** → **Signing Configs**
3. Build signed HAP

### AppGallery Connect

1. Register at https://developer.huawei.com/
2. Create app in AppGallery Connect console
3. Upload signed HAP
4. Submit for review

## Project Structure (Final)

```
GeoDB/
├── entry/
│   ├── src/main/
│   │   ├── cangjie/              # Our Cangjie backend
│   │   │   ├── geodb/
│   │   │   │   ├── ffi.cj
│   │   │   │   ├── types.cj
│   │   │   │   └── engine.cj
│   │   │   ├── ui/
│   │   │   │   └── ContentView.cj
│   │   │   └── main.cj
│   │   ├── ets/                  # ArkTS UI (if using hybrid)
│   │   │   └── pages/
│   │   │       └── Index.ets
│   │   ├── cpp/                  # CMake config
│   │   │   └── CMakeLists.txt
│   │   └── resources/            # Images, strings, etc.
│   ├── libs/
│   │   └── arm64-v8a/
│   │       └── libgeodb_ffi.so
│   └── build-profile.json5
├── oh-package.json5
└── hvigorfile.ts
```

## Resources

- **DevEco Studio Guide**: https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/
- **Cangjie Language**: https://docs.cangjie-lang.cn/en/
- **ArkUI Framework**: https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/arkui-overview-V5
- **Native Development**: https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/napi-guidelines-V5

## Next Steps

1. ✅ Create DevEco project
2. ✅ Copy Cangjie source files
3. ✅ Build and install native library
4. ✅ Create ArkUI interface
5. ⬜ Test on device/emulator
6. ⬜ Optimize performance
7. ⬜ Add error handling
8. ⬜ Prepare for AppGallery

Good luck! 🚀
