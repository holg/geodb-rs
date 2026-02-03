# Android Release Setup for GitHub Actions

This guide explains how to set up automated Android builds and Google Play Store deployment.

## Prerequisites

1. Android app signing keystore (you already have `release.keystore`)
2. Google Play Console access
3. Google Play Service Account with API access

## GitHub Secrets Configuration

You need to configure the following secrets in your GitHub repository settings:

### 1. Android Signing Secrets

#### `ANDROID_KEYSTORE_BASE64`
Your keystore file encoded in base64.

**How to generate:**
```bash
cd GeoDB-Apps/android-app
base64 -i app/release.keystore -o keystore.base64.txt
# Copy the contents of keystore.base64.txt
```

Then add it to GitHub:
- Go to: `Settings` → `Secrets and variables` → `Actions`
- Click `New repository secret`
- Name: `ANDROID_KEYSTORE_BASE64`
- Value: Paste the base64 string
- Click `Add secret`

#### `ANDROID_KEYSTORE_PASSWORD`
The password for your keystore.

**From your .env file:** `hugo16031930`

- Name: `ANDROID_KEYSTORE_PASSWORD`
- Value: `hugo16031930`

#### `ANDROID_KEY_PASSWORD`
The password for your signing key.

**From your .env file:** `hugo16031930`

- Name: `ANDROID_KEY_PASSWORD`
- Value: `hugo16031930`

---

### 2. Google Play Store Secrets

#### `PLAY_STORE_SERVICE_ACCOUNT_JSON`
Service account JSON for Google Play Console API access.

**How to create:**

1. **Go to Google Cloud Console:**
   - Visit: https://console.cloud.google.com/
   - Select your project (or create one)

2. **Enable Google Play Android Developer API:**
   - Go to `APIs & Services` → `Library`
   - Search for "Google Play Android Developer API"
   - Click `Enable`

3. **Create Service Account:**
   - Go to `IAM & Admin` → `Service Accounts`
   - Click `Create Service Account`
   - Name: `github-actions-play-store`
   - Description: `Service account for automated Play Store deployment`
   - Click `Create and Continue`
   - Skip granting roles (we'll do this in Play Console)
   - Click `Done`

4. **Create Service Account Key:**
   - Click on the newly created service account
   - Go to `Keys` tab
   - Click `Add Key` → `Create new key`
   - Choose `JSON` format
   - Click `Create`
   - A JSON file will be downloaded (e.g., `github-actions-play-store-xxxxx.json`)

5. **Grant Access in Google Play Console:**
   - Go to: https://play.google.com/console/
   - Select `Users and permissions` from left menu
   - Click `Invite new users`
   - Enter the service account email (looks like: `github-actions-play-store@PROJECT_ID.iam.gserviceaccount.com`)
   - Under `App permissions`, add your app: `eu.trahe.geodb`
   - Grant the following permissions:
     - ✅ View app information and download bulk reports
     - ✅ Manage production releases
     - ✅ Manage testing track releases
   - Click `Invite user`
   - Wait for the invitation to be accepted (automatic for service accounts)

6. **Add to GitHub Secrets:**
   - Open the downloaded JSON file
   - Copy the entire contents
   - Go to GitHub: `Settings` → `Secrets and variables` → `Actions`
   - Click `New repository secret`
   - Name: `PLAY_STORE_SERVICE_ACCOUNT_JSON`
   - Value: Paste the entire JSON content
   - Click `Add secret`

---

## Workflow Triggers

The Android Release workflow runs in these scenarios:

### 1. **On Push to Main (Testing)**
- Builds unsigned APK for testing
- Does NOT deploy to Play Store
- Uploads APK as GitHub artifact

### 2. **On Release (Production)**
- Builds signed AAB
- Builds signed APKs (all architectures)
- Deploys AAB to Google Play Store (production track)
- Attaches AAB and APKs to GitHub release

### 3. **Manual Trigger (workflow_dispatch)**
- Can be triggered manually from GitHub Actions tab
- Builds unsigned APK (same as push to main)

---

## Release Process

When you want to release a new version:

1. **Update version in build.gradle.kts:**
   ```kotlin
   versionCode = 6  // Increment
   versionName = "0.1.5"  // Update
   ```

2. **Update release notes** (optional):
   - Edit files in `GeoDB-Apps/android-app/release-notes/`
   - Supported languages: `en-US/`, `de-DE/`, `zh-CN/`, etc.
   - Each file should contain release notes (max 500 chars)

3. **Commit and push:**
   ```bash
   git add GeoDB-Apps/android-app/app/build.gradle.kts
   git commit -m "Bump version to 0.1.5"
   git push origin main
   ```

4. **Create GitHub Release:**
   ```bash
   git tag v0.1.5
   git push origin v0.1.5
   ```

   Or create via GitHub UI:
   - Go to `Releases` → `Draft a new release`
   - Tag: `v0.1.5`
   - Title: `GeoDB v0.1.5`
   - Description: Add release notes
   - Click `Publish release`

5. **Workflow will automatically:**
   - ✅ Build native Rust libraries for all Android ABIs
   - ✅ Build signed AAB
   - ✅ Build signed APKs
   - ✅ Upload AAB to Google Play Store (production)
   - ✅ Attach AAB and APKs to GitHub release

---

## Troubleshooting

### Build fails: "NDK not found"
- The workflow installs NDK r26c automatically
- If issues persist, check `ANDROID_NDK_HOME` environment variable

### Build fails: "Keystore not found"
- Check that `ANDROID_KEYSTORE_BASE64` secret is correctly set
- Verify base64 encoding has no line breaks

### Play Store upload fails: "401 Unauthorized"
- Check that service account email is added to Play Console
- Verify service account has correct permissions
- Ensure `PLAY_STORE_SERVICE_ACCOUNT_JSON` is valid JSON

### Play Store upload fails: "Version code already exists"
- You must increment `versionCode` for each release
- Google Play tracks version codes globally - you cannot reuse them

### Release notes not showing up
- Check that release notes files exist in `GeoDB-Apps/android-app/release-notes/`
- Filename format: `<locale>/default.txt` (e.g., `en-US/default.txt`)
- Ensure files are UTF-8 encoded
- Maximum 500 characters per file

---

## File Locations

- **Workflow:** `.github/workflows/android-release.yml`
- **Build config:** `GeoDB-Apps/android-app/app/build.gradle.kts`
- **Release notes:** `GeoDB-Apps/android-app/release-notes/<locale>/default.txt`
- **Keystore (local only, gitignored):** `GeoDB-Apps/android-app/app/release.keystore`
- **Build script:** `scripts/build_android_release.sh`

---

## Security Notes

⚠️ **NEVER commit these files:**
- `.env` (contains keystore passwords)
- `*.keystore` (signing key)
- `service-account.json` (Play Store API key)

These are already in `.gitignore` but double-check before committing!

---

## Testing Before Release

To test the build process locally:

```bash
./scripts/build_android_release.sh
```

This will:
1. Build Rust native libraries
2. Copy to Android app
3. Build signed AAB (if keystore configured)
4. Output to `releases/android/`

---

## Monitoring

- **GitHub Actions:** Check workflow runs at `Actions` tab
- **Play Store:** Monitor release status in Play Console → `Release` → `Production`
- **Build artifacts:** Download from GitHub Actions workflow run (unsigned builds)
- **GitHub Releases:** Signed AAB/APKs attached automatically

---

## Quick Reference: Secrets Needed

| Secret Name | Value | Where to Find |
|-------------|-------|---------------|
| `ANDROID_KEYSTORE_BASE64` | Base64 of keystore file | `base64 -i app/release.keystore` |
| `ANDROID_KEYSTORE_PASSWORD` | Keystore password | From your `.env` file |
| `ANDROID_KEY_PASSWORD` | Key password | From your `.env` file |
| `PLAY_STORE_SERVICE_ACCOUNT_JSON` | Service account JSON | Google Cloud Console |

---

## Next Steps

1. ✅ Create GitHub secrets (see above)
2. ✅ Test workflow with manual trigger
3. ✅ Create a test release to verify end-to-end
4. ✅ Monitor Play Store for successful deployment

For questions or issues, see: https://github.com/holg/geodb-rs/issues
