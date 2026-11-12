> [!WARNING]
> This is a prototype, reference implementation, and proof-of-concept. This open source code is developed and published by Parity Technologies for research, experimentation, and developer education only. It has not been audited, is actively experimental, and may contain bugs, vulnerabilities, or incomplete features. The app is a self-custodial wallet that can hold real assets — use at your own risk.

## Build Project

Clone [paritytech/polkadot-android-community](https://github.com/paritytech/polkadot-android-community)
and follow the public setup instructions in the root [README](../README.md).
Build-time configuration, signing, and distribution requirements are documented in
[docs/DEPLOYMENT.md](../docs/DEPLOYMENT.md). Never commit keystores,
`google-services.json`, service-account files, or credentials.

## Utility Scripts

### Generate BIP39 Mnemonic
Generate a random 12-word mnemonic for testing:
```bash
python3 scripts/generate-mnemonic.py
```

# CI/CD Workflows

This document describes all continuous integration and delivery flows for the Polkadot Android application.

## Repository Configuration

Configure these values under **Settings → Secrets and variables → Actions**. GitHub
does not expose repository configuration to a runner automatically; the workflows
under `.github/workflows` explicitly map build values to the environment read by
Gradle and notification values to action inputs.

### Build Variables

These values are public application configuration and are intentionally stored as
GitHub Actions Variables rather than Secrets. They are embedded in the APK and must
not contain credentials.

Most variables below are **mandatory**: the build reads them with `readSecretOrThrow`
and fails at configuration time when one is missing or empty, so a deploy can never ship
a placeholder fallback. Only `signingConfigs` still uses `readSecretOrDefault`.
`SENTRY_DSN` is optional and keeps a fallback.

| Variable | Purpose |
|----------|---------|
| `APPLICATION_ID` | Base Android application ID. The build adds `.debug`, `.nightly` or `.safetynet` for those build types. Every resulting id must match a client in `google-services.json`. |
| `APPLICATION_NAME` | Launcher name of the application. `DEBUG_APPLICATION_NAME`, `NIGHTLY_APPLICATION_NAME` and `SAFETYNET_APPLICATION_NAME` optionally override it per build type; when unset they are derived from this value. |
| `IOS_BUNDLE_ID` | Bundle ID of the iOS app, sent as the APNs topic when a chat push goes to an iOS peer. Debug builds append `.develop`, safetynet builds `.safety`. |
| `PRIVACY_POLICY_URL` | Privacy-policy destination shown by the application. |
| `CURRENCY_SYMBOL` | Symbol of the in-app digital currency shown in the UI — card title, send/get actions, and every formatted amount. |
| `FIAT_SYMBOL` | Fiat symbol prefixed to formatted amounts. `$` also puts the dollar icon on the chat pay button; any other value shows the neutral cash icon. |
| `TERMS_OF_USE_URL` | Terms-of-use destination shown by the application. |
| `LOG_COLLECTION_EMAIL` | Recipient used by the debug log-sharing flow. |
| `CONTACT_EMAIL` | Recipient of the Contact us action on the Legal & Support screen. |
| `SENTRY_DSN` | Client DSN embedded in debug/nightly manifests for runtime error reporting. Optional; an empty value disables runtime reporting. |
| `SENTRY_ORG` | Sentry organization slug used by the Gradle plugin. |
| `SENTRY_PROJECT` | Sentry project slug used by the Gradle plugin. |

### Workflow Variables

These values configure CI notifications and are not consumed by the Android build.

| Variable | Purpose |
|----------|---------|
| `CI_MATRIX_ROOM_IDS` | Comma-separated Matrix room IDs that receive nightly release notifications. |
| `NIGHTLY_DOWNLOAD_LINKS` | Multiline Markdown list of download links included in nightly release notifications. |

### Build Secret

| Secret | Purpose |
|--------|---------|
| `NIGHTLY_FUNDING_MNEMONIC` | Funding account used by nightly and production test contours. It is provided only to Gradle build/test steps. |

`NIGHTLY_FUNDING_MNEMONIC` is protected while stored by GitHub and is masked in
workflow logs. The current application places it in `BuildConfig`, however, so it
can be extracted from a distributed APK. Use only a tightly funded test account;
never use a treasury, production, or otherwise valuable mnemonic here.

See [Deployment §5](../docs/DEPLOYMENT.md#5-environment-variables--secrets-reference)
for signing, Google/Firebase, Sentry, publishing, and local-build configuration.

## Flows Overview

### 1. Pull Request Validation Flow
**Trigger:** Pull requests to any branch (except release branches)  
**Purpose:** Validate code changes through automated testing

**Steps:**
1. Check for `skip-ci` label
2. Setup Android development environment
3. Run unit tests
4. Run build

**Workflows:**
- [`pr.yml`](workflows/pr.yml)

---

### 2. Development Build Distribution Flow
**Trigger:** Manual dispatch or PR merge to `main` branch  
**Purpose:** Distribute development builds to QA team via Firebase

**Steps:**
1. Setup Android environment
2. Calculate and update build number (10100 + run_number)
3. Build app with Debug configuration
4. Upload to Firebase App Distribution
5. Notify configured groups

**Workflows:**
- [`firebase_debug_distribution.yml`](workflows/firebase_debug_distribution.yml)

**Configuration:**
- Build type: Debug
- Default groups: `android-dev-testers`

---

### 3. Production Release Flow
**Trigger:** Manual workflow dispatch  
**Purpose:** Prepare and distribute production releases to Firebase

**Steps:**

#### Phase 1: Release Preparation
1. Validate user permissions (optional)
2. Create release branch from source ref (default: `main`)
3. Optionally bump version (major/minor/patch/no-bump)
4. Commit version changes to release branch
5. Create pull request to `main`
6. Trigger Firebase Release workflow

#### Phase 2: Firebase Distribution (triggered automatically or by PR updates)
1. Security verification (only bot-initiated PRs allowed)
2. Increment build number in Release configuration
3. Commit build number update
4. Run tests
5. Build and upload to Firebase
6. Comment on PR with build information

#### Phase 3: Backport to Source Branch (triggered after PR merge)
1. Extract source branch metadata from merged PR
2. Validate source branch exists
3. Create backport PR: `release-{version}` → `source_ref` (e.g., `main`)
4. Include incremented build numbers and any hotfixes from release branch

**Workflows:**
- [`release_prepare.yml`](workflows/release_prepare.yml) - Phase 1
- [`firebase_release_distribution.yml`](workflows/firebase_release_distribution.yml) - Phase 2 & 3

**Configuration:**
- Build type: Release
- Branches: `release-{version}` → `main` → backport to `source_ref`
- Source branch tracking: Embedded in PR metadata

---

## Version and Build Number Management

### Debug Builds
- **Version:** Read from `Versions.kt` (not changed)
- **Build number:** `10000 + github.run_number`

### Release Builds
- **Version:**
  - Format: `X.Y.Z` (major.minor.patch)
  - Updated by `release_prepare.yml` based on bump level
  - Stored in `Versions.kt` → `DefaultVersionName`
- **Build number:**
  - Auto-incremented by `firebase_release_distribution.yml`
  - Stored in `Versions.kt` → `DefaultVersionCode`

---

## Firebase Distribution

### Debug Builds
- **App ID:** `ANDROID_FIREBASE_APP_ID` (from secrets)
- **Groups:** `dev-team`
- **APK:** Debug variant with debug keystore

### Release Builds
- **App ID:** `ANDROID_FIREBASE_RELEASE_APP_ID` (from secrets)
- **Groups:** `dev-team`
- **APK:** Release variant with release keystore

### Nightly Builds
Each build type carries its own `applicationIdSuffix`, so App Distribution treats it
as a separate Firebase app and needs its own App ID secret.

| Variant | App ID | Groups |
|---------|--------|--------|
| `gpNightly` | `ANDROID_FIREBASE_NIGHTLY_APP_ID` | `CI_FIREBASE_GROUP` |
| `vanillaNightly` | `ANDROID_FIREBASE_NIGHTLY_APP_ID` (product flavors add no suffix) | `CI_FIREBASE_GROUP_VANILLA` |
| `gpSafetynet` | `ANDROID_FIREBASE_SAFETYNET_APP_ID` | `CI_FIREBASE_GROUP_SAFETYNET` |

---

## Security

### Release Workflows
Both `release_prepare.yml` and `firebase_release_distribution.yml` include security checks:

1. **`release_prepare.yml`:**
  - Only authorized users can run (optional, can be enabled in job condition)

2. **`firebase_release_distribution.yml`:**
  - `workflow_dispatch`: Must be triggered by `github-actions[bot]`
  - `pull_request`: PR must be created by `github-actions[bot]`

This ensures release builds can only be initiated through the official release process.

---

## Build Artifacts Storage (S3)

All Android builds are automatically uploaded to Scaleway Object Storage for archival and distribution.

**Bucket:** `polkadot-app-artefacts` (region: `fr-par`)

### File Naming Scheme

| Workflow | Path Pattern | Static Path | Example URL |
|----------|-------------|-------------|-------------|
| Debug (Firebase) | `/android/debug/polkadot-app-{version}-{build}.apk` | `/android/debug/polkadot-app.apk` | `http://polkadot-app-artefacts.s3.fr-par.scw.cloud/android/debug/polkadot-app-1.2.3-10150.apk` |
| Release (Firebase via PR) | `/android/releases/polkadot-app-{version}-{build}.apk` | `/android/releases/polkadot-app.apk` | `http://polkadot-app-artefacts.s3.fr-par.scw.cloud/android/releases/polkadot-app-1.0.0-456.apk` |
| Nightly Release | `/android/nightly/polkadot-app-{version}-{build}.apk` | `/android/nightly/polkadot-app.apk` | `http://polkadot-app-artefacts.s3.fr-par.scw.cloud/android/nightly/polkadot-app-1.0.0-1456.apk` |

**Static paths** always point to the latest build from that workflow, while **versioned paths** preserve all historical builds.

---

## Scripts

Version management scripts located in `.github/scripts/`:

- **`read_versions.py`** - Reads current version and build number from `Versions.kt`
- **`update_marketing_version.py`** - Updates version (DefaultVersionName) in `Versions.kt`
- **`update_build_number.py`** - Updates or increments build number (DefaultVersionCode) in `Versions.kt`

All scripts work with `build-logic/convention/src/main/kotlin/Versions.kt` file.
