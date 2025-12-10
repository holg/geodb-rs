# Adding Cangjie to Existing ArkTS Project

This guide shows how to add Cangjie backend to your existing ArkTS-based HarmonyOS project.

## Current Project Structure (ArkTS)

```
GeoDB/
├── entry/
│   └── src/main/
│       ├── ets/          ← ArkTS UI code
│       ├── resources/    ← Assets
│       └── module.json5
```

## Target Structure (Hybrid)

```
GeoDB/
├── entry/
│   ├── src/main/
│   │   ├── ets/          ← ArkTS UI code
│   │   ├── cpp/          ← Native C++ bridge (new)
│   │   └── resources/
│   └── libs/             ← Native libraries (new)
│       └── arm64-v8a/
│           └── libgeodb_ffi.so
```

## Approach: Use NAPI (Node-API) Bridge

Since Cangjie support is still maturing in DevEco Studio, the **most practical approach** is:

1. **Rust** → C API (already done in `c_api.rs`)
2. **C++/NAPI** → Bridge Rust to ArkTS
3. **ArkTS** → UI and app logic

This gives you:
- ✅ Works with current DevEco Studio
- ✅ Native performance (no overhead)
- ✅ Can still use our Cangjie code later
- ✅ Well-documented approach

## Alternative: Wait for Full Cangjie Support

If you want pure Cangjie:
- Wait for DevEco Studio Cangjie plugin maturity
- Or use standalone Cangjie toolchain for CLI version
- Then integrate when plugin is stable

## Recommendation

For **now**, let's use the **NAPI bridge** approach since:
1. It works with your current DevEco setup
2. Same performance as Cangjie (both call C directly)
3. You can migrate to pure Cangjie later
4. More documentation available

Would you like me to:
1. Create the NAPI bridge layer?
2. Or help you install Cangjie plugin for DevEco Studio?
