# Release Workflow Documentation

This document describes the automated release process for GeoDB.

## Release Workflow Overview

When you push a version tag (e.g., `v0.1.5`), the following workflows run in parallel:

```
Tag Push (v0.1.5)
├─ create-release.yml       → Creates GitHub Release with notes
├─ release-binaries.yml     → Builds CLI binaries → Uploads to Release
├─ pypi.yml                 → Publishes Python wheels → Uploads to Release
├─ android-release.yml      → Builds Android app → Play Store + Release
└─ publish-crates.yml       → Publishes to crates.io
```

## Release Artifact Organization

All artifacts are categorized and uploaded to the GitHub Release:

### 📦 CLI Binaries (`release-binaries.yml`)
```
CLI/
├── README.md
├── geodb-cli-x86_64-unknown-linux-gnu.tar.gz
├── geodb-cli-x86_64-apple-darwin.tar.gz
├── geodb-cli-aarch64-apple-darwin.tar.gz
└── geodb-cli-x86_64-pc-windows-msvc.zip
```

### 🐍 Python Wheels (`pypi.yml`)
```
Python/
├── README.md
├── geodb_rs-VERSION-manylinux_*_x86_64.whl
├── geodb_rs-VERSION-macosx_*_x86_64.whl
├── geodb_rs-VERSION-macosx_*_arm64.whl
└── geodb_rs-VERSION-win_amd64.whl
```

### 📱 Android App (`android-release.yml`)
```
Android/
├── README.md
├── geodb-android-release.apk
└── app-release.aab
```

### 📦 Rust Crates (`publish-crates.yml`)
Published to crates.io (not uploaded to GitHub Release):
- `geodb-core`
- `geodb-cli`
- `geodb-wasm`

## Release Process

### 1. Prepare for Release

Ensure version numbers are updated in:
- `crates/*/Cargo.toml` - All crate versions
- `GeoDB-Apps/android-app/app/build.gradle.kts` - versionCode and versionName
- `CHANGELOG.md` - Document changes (optional but recommended)

### 2. Run CI Tests

```bash
# Make changes, commit
git add .
git commit -m "Prepare v0.1.5 release"

# Push and wait for CI to pass
git push origin main

# Wait for CI workflow to complete successfully
```

### 3. Create and Push Release Tag

```bash
# Create annotated tag
git tag -a v0.1.5 -m "Release v0.1.5"

# Push tag to trigger all release workflows
git push origin v0.1.5
```

### 4. Monitor Workflows

Go to GitHub Actions and monitor:
1. **create-release** - Creates the GitHub Release
2. **release-binaries** - Builds and uploads CLI binaries
3. **pypi** - Publishes Python wheels and uploads to Release
4. **android-release** - Builds Android app, deploys to Play Store, uploads to Release
5. **publish-crates** - Publishes Rust crates to crates.io

All workflows run independently and in parallel.

### 5. Verify Release

Check that the GitHub Release contains:
- ✅ CLI binaries for all platforms (with README)
- ✅ Python wheels for all platforms (with README)
- ✅ Android APK and AAB (with README)
- ✅ Release notes with download instructions

Verify publications:
- ✅ [crates.io](https://crates.io/crates/geodb-core)
- ✅ [PyPI](https://pypi.org/project/geodb-rs)
- ✅ [Google Play Store](https://play.google.com/store/apps/details?id=eu.trahe.geodb)

## Manual Workflow Triggers

All release workflows support manual dispatch from GitHub Actions UI:

- **PyPI**: Can manually trigger wheel builds and publication
- **Crates**: Can manually publish to crates.io
- **Binaries**: Can manually build and upload CLI binaries
- **Android**: Can build without Play Store deployment

## CI vs Release Workflows

### CI Workflow (`ci.yml`)
- **Triggers**: Push to main, PRs
- **Purpose**: Fast feedback for development
- **Builds**:
  - ✅ Lint, test, docs
  - ✅ CLI binaries (all platforms)
  - ✅ ONE test Python wheel (Linux x86_64 only)
- **Does NOT**: Publish or create releases

### Release Workflows
- **Triggers**: Tag push (`v*`), manual dispatch
- **Purpose**: Production releases
- **Builds**: All platforms, all artifacts
- **Does**: Publish to package registries and create GitHub Release

## Workflow Files

| File | Purpose | Triggers |
|------|---------|----------|
| `ci.yml` | CI testing | Push to main, PRs |
| `create-release.yml` | Create GitHub Release | Tag push |
| `release-binaries.yml` | Build CLI for all platforms | Tag push, manual |
| `pypi.yml` | Publish Python wheels | Tag push, manual |
| `android-release.yml` | Build & deploy Android | Tag push, manual |
| `publish-crates.yml` | Publish to crates.io | Tag push, manual |

## Troubleshooting

### Release workflow failed but tag was pushed

You can manually trigger the failed workflow from GitHub Actions UI. All workflows support `workflow_dispatch`.

### Version mismatch

The `publish-crates.yml` workflow validates that `Cargo.toml` versions match the git tag. If mismatch is detected, the workflow fails with a clear error message.

### Artifact not found

If a dependent job can't find artifacts, check that:
1. The previous job completed successfully
2. The artifact names match between upload and download steps
3. The `if` conditions are correct (e.g., `github.ref_type == 'tag'`)

### Play Store deployment failed

The Android workflow separates building from deployment. The build always runs, but Play Store upload only happens if:
- Tag was pushed (`github.ref_type == 'tag'`), OR
- Manual dispatch with `deploy_to_play_store: true`

Check the `deploy-play-store` job logs for specific Play Store API errors.

## Required Secrets

Configure these in GitHub repository settings → Secrets and variables → Actions:

### Android
- `ANDROID_KEYSTORE_BASE64` - Base64-encoded keystore file
- `ANDROID_KEYSTORE_PASSWORD` - Keystore password
- `ANDROID_KEY_PASSWORD` - Key password
- `PLAY_STORE_SERVICE_ACCOUNT_JSON` - Google Play service account JSON

### Crates.io
- `CARGO_REGISTRY_TOKEN` - crates.io API token

### PyPI
- `PYPI_API_TOKEN` - PyPI API token

### GitHub
- `GITHUB_TOKEN` - Automatically provided by GitHub Actions
