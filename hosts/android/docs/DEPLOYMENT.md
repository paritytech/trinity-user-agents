# Deployment Guide

This document explains **how to build, sign and publish** the Polkadot Android
Community application, and **which environment variables / secrets** you need to
configure.

It is intentionally **generic**. This repository ships GitHub Actions for the
maintained build and distribution flows, while this guide describes the
*building blocks* so that a fork or a community deployment can use its own
credentials.

> **Scope:** this is the manual / reference guide for the workflows under
> `.github/workflows` and for custom deployments.

---

## 1. How publishing works (overview)

A release is just a **signed APK/AAB** of a chosen build variant, delivered through
one or more channels. The app is built with Gradle, has native Rust components
(built via the NDK), and reads all credentials from `local.properties` **or**
environment variables (see §5). The typical flow is:

1. Provision the toolchain (§2).
2. Provide a signing keystore (§3), a Firebase `google-services.json` (§4) and the Remote Config values (§4.1) — both editions need them, because Remote Config is required for the app to run.
3. Set the required environment variables / secrets (§5). Gradle refuses to configure the project while any mandatory one is missing.
4. Build the edition you want to ship (§6).
5. Deliver the artifact through a channel of your choice (§7):
   Google Play, Firebase App Distribution, GitHub Releases, object storage, or a
   direct APK link.

### Distribution editions

The app ships two parallel distribution tracks:

| Edition | Target | Services | Channel |
|---------|--------|----------|---------|
| `gp` | Standard Android / Google Play | Google Mobile Services (GMS), Firebase, Play Integrity | Google Play Store (AAB/APK) |
| `vanilla` | **GrapheneOS** and other GMS-free environments | No Google Play Services; Firebase Remote Config only | Sideloadable APK (GitHub Releases, direct link, etc.) |

The `vanilla` edition is a first-class release track aimed at privacy-focused users
running [GrapheneOS](https://grapheneos.org/). It carries the same core app
functionality and is fully operational without Google Play Services. Google Sign-In,
Firebase Auth, Firebase Analytics, and Firebase Crashlytics are excluded from the
`vanilla` APK.

Both editions do initialize the default `FirebaseApp`, because **Firebase Remote
Config is required for the app to function** (`tools/remoteconfig:impl` drives feature
gates, endpoints, and transaction extension versions). Remote Config does not depend on
Google Play Services being present on the device — it reaches the Firebase REST
endpoints directly and identifies the install through Firebase Installations — so it
works on GrapheneOS. Firebase Messaging and Firebase App Check remain compile-time
transitive dependencies (from `tools/push-notifications:impl` and
`tools/integrity:impl`) but stay inert in `vanilla`: FCM token auto-init is switched
off in `app/src/vanilla/AndroidManifest.xml`, and App Check has no injection sites.

The `vanilla` edition is distributed as a standalone APK alongside, not instead of,
the Google Play version.

---

## 2. Toolchain / prerequisites

These versions are what the project is known to build with (they mirror the
previously-used CI environment):

| Tool            | Version                | Notes                                             |
|-----------------|------------------------|---------------------------------------------------|
| JDK             | 21 (Temurin)           | Required by AGP / Kotlin                           |
| Android SDK     | latest                 | Plus build-tools matching the project             |
| Android NDK     | r29                    | Needed for the native Rust bindings               |
| Clang           | 21                     | Native toolchain                                  |
| Rust            | stable                 | + targets below                                   |
| `cargo-ndk`     | latest                 | `cargo install cargo-ndk`                         |
| Node.js         | 24                     | Required: `feature/products` runs `npm install` / `npm run build` for the dApp container during the Gradle build |
| Python          | 3.13                   | Required by the Rust Android Gradle plugin (linker wrapper); also used by the version scripts (§6) |

Rust targets:

```bash
rustup target add aarch64-linux-android
rustup target add armv7-linux-androideabi
rustup target add x86_64-linux-android
rustup target add i686-linux-android
```

Point Gradle at the NDK by adding to `local.properties`:

```properties
ndk.dir=/path/to/android-ndk-r29
sdk.dir=/path/to/android-sdk
```

When `cargo`, `rustc` or `python` are not on the PATH Gradle sees (typical for IDE-launched
builds on macOS), point the Rust plugin at them in the same file:

```properties
rust.cargoCommand=/Users/me/.cargo/bin/cargo
rust.rustcCommand=/Users/me/.cargo/bin/rustc
rust.pythonCommand=/usr/bin/python3
```

`developer-tools/setup.sh` only installs the Detekt pre-commit hook; it does not install
any part of the toolchain.

---

## 3. Signing

The app defines two signing configs in `app/build.gradle.kts`:

- **`dev`** — used by the `debug`, `nightly` and `safetynet` build types.
- **`release`** — used by the `release` build type.

Every `assemble*` task signs its output, so even a local debug build needs the dev
keystore and its three `CI_KEYSTORE_*` values (§5.1).

### 3.1 Generate a keystore

```bash
# dev keystore (debug / nightly / safetynet)
keytool -genkeypair -v \
  -keystore develop_key.jks \
  -alias my-dev-key \
  -keyalg RSA -keysize 2048 -validity 10000

# release keystore
keytool -genkeypair -v \
  -keystore release_key.jks \
  -alias my-release-key \
  -keyalg RSA -keysize 2048 -validity 10000
```

### 3.2 Where the keystore file lives

By default the build looks for the files relative to the repo root:

- dev: `develop_key.jks`
- release: `release_key.jks`

You can override the path with `DEV_KEYSTORE_FILE` / `RELEASE_KEYSTORE_FILE`
(see §5). In CI it is common to store the keystore base64-encoded in a secret and
decode it before the build:

```bash
echo "$RELEASE_KEYSTORE_BASE64" | base64 --decode > release_key.jks
```

### 3.3 Passwords / alias

Provided via the signing environment variables in §5
(`*_KEYSTORE_PASS`, `*_KEYSTORE_KEY_ALIAS`, `*_KEYSTORE_KEY_PASS`).

---

## 4. `google-services.json` (Firebase)

> **Required by both editions.** Firebase Remote Config is mandatory for the app to
> run, so `vanilla`/GrapheneOS builds need this file too. Without it the Google
> Services plugin fails the build.

Place `google-services.json` in the `app` module root so that a single file covers
every flavor and build type:

```
app/google-services.json
```

The module root is the plugin's fallback location, so `gpDebug`, `gpNightly`,
`gpRelease` and their `vanilla` counterparts all resolve to it. A flavor-specific
`app/src/gp/google-services.json` still takes precedence for `gp` variants if you
need to point the two editions at different Firebase projects.

`vanilla` needs no separate Firebase app registration: product flavors add no
`applicationIdSuffix` (only build types do), so both editions share the same
application ids and therefore the same `google-services.json` entries. The file's
client list must contain every application id the build produces: `APPLICATION_ID`
itself (release) plus its `.debug`, `.nightly` and `.safetynet` variants.

Obtain it from **your own** Firebase project (Project settings → Your apps →
Android app → download `google-services.json`). In CI decode it into this path
before the build:

```bash
echo "$GOOGLE_SERVICES_JSON_BASE64" | base64 --decode > app/google-services.json
```

Firebase services the app uses, so you can enable them in that project:

| Service | Editions | Used for |
|---------|----------|----------|
| Remote Config | both | All runtime configuration (§4.1). Mandatory. |
| Authentication (Google provider) + Firestore | `gp` | Cloud backups: the encryption-key record is stored in the `users-<suffix>` collection (`-debug`, `-nightly`, `-safetynet`, `-production`) of the database named by `FIRESTORE_DATABASE_ID`. Security rules must let a signed-in user read/write their own document. |
| Cloud Messaging | `gp` | Push notifications. |
| Crashlytics, Analytics | `gp` | Crash and usage reporting. |

### 4.1 Remote Config keys

`tools/remoteconfig/impl` ships an **empty** defaults file, so every value below has to
exist in your Firebase project's Remote Config; an unset key reads back as an empty
string. The chain list is the first thing the app loads after start-up, and it fails
without it. JSON values are stored as Remote Config strings.

| Key | Type | Needed for | Read by | Description |
|-----|------|------------|---------|-------------|
| `chains` | JSON array | app start-up (`release`) | `chains` — `RemoteConfigChainFetcher` | Chain set for `release` builds (`PRODUCTION` environment). |
| `chains_v2` | JSON array | app start-up (`debug`, `nightly`, `safetynet`) | `chains` — `RemoteConfigChainFetcher` | Chain set for the testnet environments. Same schema as `chains`. |
| `dot_ns_config` | JSON object | dApps, usernames, funding | `feature/dotns`, `feature/dotns-gateway` | `{ "resolverContractAddress", "registryContractAddress", "popControllerAddress", "popResolverAddress" }` — hex contract addresses of the DotNS deployment. `registryContractAddress` may be omitted; then only legacy name resolution works. |
| `identity_backend_url` | string (URL) | username registration, Proof-of-Unique-Device | `app` — `RemoteConfigIdentityBackendUrlProvider` | Base URL of your deployment of [device-uniqueness-backend-community](https://github.com/paritytech/device-uniqueness-backend-community). Every identity-backend request is rewritten to this host at runtime. |
| `transaction_extension_versions` | JSON object | signing on People Chain / Asset Hub | `feature/transactions` | Map `chainId → transaction extension version` (number). A chain missing from the map uses version `0`. |
| `funding_config` | JSON object | on-ramp / off-ramp funding dApps | `feature/products` | `{ "onrampUrl": "…", "offrampUrl": "…" }` — URLs or DotNS names of the on-ramp and off-ramp products (`https://` is assumed when the scheme is missing). Both fields are required. |
| `cross_chain_transfers` | JSON object | cross-chain transfers | `feature/cross-chain-transfers` | `{ "chains": [...], "customTeleports": [...] }` — XCM transfer directions. Schema: `feature/cross-chain-transfers/impl/.../data/model/DynamicCrossChainConfigRemote.kt`. |
| `coinage_instance_id` | string (unsigned integer) | Coinage | `feature/coinage` | Instance id of the Coinage deployment. |
| `account_data_store_config` | JSON object | Coinage | `feature/coinage` | `{ "contractAddress": "0x…" }` — the account data store contract. |
| `payment_asset_config` | JSON object | payment asset branding (optional) | `feature/tokens` — `RealPaymentAssetBrandProvider` | `{ "symbol": "CASH", "iconSquareUrl": "https://…/square.svg", "iconWideUrl": "https://…/wide.svg" }` — the payment asset's symbol and logos: a square mark for amounts and chat payments, a wide mark-plus-wordmark for the balance card, as absolute `http(s)` URLs (SVG or PNG). Every field is optional; anything missing or failing to load falls back to `CURRENCY_SYMBOL` and the built-in mark. The last activated value applies at start-up and the object is re-read right after the first sync; logos load on display through the app image loader and are cached by URL, so publish a changed logo under a new URL. |
| `collectibles_enabled` | boolean | collectibles (optional) | `feature/videogame` | Feature gate for the collectibles webview. |
| `collectibles_fallback_url` | string (URL) | collectibles (optional) | `feature/videogame` | Fallback URL of the collectibles webview when DotNS resolution fails. |
| `app_sharing_url` | string (URL) | Share ID on the Pocket tab | `feature/wallet` — `RealAppSharingConfigRepository` | Download link placed in the invite text the ID card shares ("Download it at {link} and add me – my username is {username}."). A plain URL, not JSON-quoted. The share waits briefly for Remote Config to sync; when it does not sync in time or the key is empty, the share reports an error instead of sending a message without a link. |

A chain descriptor in `chains` / `chains_v2` is a JSON object deserialised into
`chains/src/main/java/io/paritytech/polkadotapp/chains/multiNetwork/chain/remote/model/ChainRemote.kt`
(`chainId`, `genesisHash`, `name`, `addressPrefix`, `nodes[{url,name}]`, `assets[]`,
`explorers[]`, `options[]`, `types`, `parentId`, `additional`); the nested models sit next to it.

---

## 5. Environment variables / secrets reference

All build-time configuration is read by the helpers in
`build-logic/convention/src/main/kotlin/Secrets.kt`, with this lookup order:

1. a key in **`local.properties`**, then
2. an **environment variable** of the same name.

An empty value counts as missing. Three helpers exist:

- `readSecretOrThrow` — **mandatory**: a missing value fails Gradle configuration
  (even `./gradlew help`) with `Missing secret '<NAME>'`. Because Gradle configures
  every module and every flavor regardless of the task you run, these are needed for
  **any** build, `vanilla` included.
- `readSecretOrDefault` — falls back to a default (only the signing passwords).
- `readSecretOrNull` — optional; `null` selects the built-in fallback.

The mandatory set, in the order Gradle reports them when absent:
`APPLICATION_ID`, `APPLICATION_NAME`, `CONTACT_EMAIL`, `LOG_COLLECTION_EMAIL`,
`PRIVACY_POLICY_URL`, `TERMS_OF_USE_URL`, `SENTRY_ORG`, `SENTRY_PROJECT`, `CURRENCY_SYMBOL`,
`FIAT_SYMBOL`, `NIGHTLY_FUNDING_MNEMONIC`, `GOOGLE_OAUTH_ID`, `FIRESTORE_DATABASE_ID`,
`GOOGLE_PROJECT_ID`.

In GitHub Actions, non-sensitive values from this section can be mapped from GitHub
variables while credentials and mnemonics must be mapped from GitHub secrets.

### 5.1 Signing (required to build a signed variant)

| Variable                   | Used by                   | Required | Description                                  |
|----------------------------|---------------------------|----------|----------------------------------------------|
| `CI_KEYSTORE_PASS`         | `app` dev signingConfig   | yes (dev)| Store password for the dev keystore          |
| `CI_KEYSTORE_KEY_ALIAS`    | `app` dev signingConfig   | yes (dev)| Key alias for the dev keystore               |
| `CI_KEYSTORE_KEY_PASS`     | `app` dev signingConfig   | yes (dev)| Key password for the dev keystore            |
| `RELEASE_KEYSTORE_PASS`    | `app` release signingConfig| yes (rel)| Store password for the release keystore      |
| `RELEASE_KEYSTORE_KEY_ALIAS`| `app` release signingConfig| yes (rel)| Key alias for the release keystore          |
| `RELEASE_KEYSTORE_KEY_PASS`| `app` release signingConfig| yes (rel)| Key password for the release keystore        |
| `DEV_KEYSTORE_FILE`        | `app` dev signingConfig   | no       | Override path to dev keystore (default `../develop_key.jks`) |
| `RELEASE_KEYSTORE_FILE`    | `app` release signingConfig| no      | Override path to release keystore (default `../release_key.jks`) |

> Build types: `debug`/`nightly`/`safetynet` use the **dev** config, `release` uses the
> **release** config. Missing passwords default to empty so project configuration
> and unit tests can run, but every `assemble*` task signs its APK and fails without
> the matching keystore file and values.

### 5.2 App API keys (consumed by the build via `buildConfigField`)

| Variable           | Used by (module)                           | Required | Description                          |
|--------------------|--------------------------------------------|----------|--------------------------------------|
| `GOOGLE_OAUTH_ID`  | `tools/auth/impl` `gp` product flavor      | yes      | OAuth 2.0 **web** client id of your Google Cloud project; passed to Google Sign-In as the ID-token audience (`requestIdToken`). |
| `GOOGLE_PROJECT_ID`| `tools/integrity/impl` `gp` product flavor | yes      | Google Cloud **project number** (integer) handed to Play Integrity as the cloud project number. |

Both are compiled into the `gp` flavor only — `vanilla` uses a no-op sign-in and `0` —
but the `gp` flavor block is still evaluated when configuring a `vanilla` build, so
both variables must be present for any build. For a `vanilla`-only fork any
non-empty value works (`GOOGLE_PROJECT_ID` must still parse as an integer).

#### 5.2.1 App identity, endpoints and values

Depending on the consumer, these become the application ID, a manifest placeholder,
or a `BuildConfig` field. Mandatory ones have no fallback; optional ones list theirs.

| Variable                    | Used by (module)              | Required | Fallback                       | Description                                                                 |
|-----------------------------|-------------------------------|----------|--------------------------------|-----------------------------------------------------------------------------|
| `APPLICATION_ID`            | `app`                         | yes      | —                              | Installed application ID; build types append `.debug` / `.nightly` / `.safetynet` |
| `APPLICATION_NAME`          | `app`                         | yes      | —                              | Base/release launcher name                                                   |
| `DEBUG_APPLICATION_NAME`    | `app` debug build             | no       | `[Debug] <APPLICATION_NAME>`   | Debug launcher name                                                          |
| `NIGHTLY_APPLICATION_NAME`  | `app` nightly build           | no       | `<APPLICATION_NAME>`           | Nightly launcher name                                                        |
| `SAFETYNET_APPLICATION_NAME`| `app` safetynet build         | no       | `[Safetynet] <APPLICATION_NAME>` | Safetynet launcher name                                                    |
| `PRIVACY_POLICY_URL`        | `app`                         | yes      | —                              | Privacy-policy destination                                                   |
| `TERMS_OF_USE_URL`          | `app`                         | yes      | —                              | Terms-of-use destination                                                     |
| `LOG_COLLECTION_EMAIL`      | `app`                         | yes      | —                              | Recipient address for the in-app "collect logs" debug share action          |
| `CONTACT_EMAIL`             | `app`                         | yes      | —                              | Recipient address for the "Contact us" action on the Legal & Support screen |
| `CURRENCY_SYMBOL`           | `common`                      | yes      | —                              | Symbol of the in-app digital currency rendered in the UI                     |
| `FIAT_SYMBOL`               | `common`                      | yes      | —                              | Fiat symbol prefixed to amounts; `$` also puts the dollar icon on the chat pay button, any other value the neutral cash icon |
| `FIRESTORE_DATABASE_ID`     | `tools/backup/impl`           | yes      | —                              | Firestore database holding the backup encryption-key records (§4); `(default)` for the project's default database |
| `NIGHTLY_FUNDING_MNEMONIC`  | `feature/transactions/impl`   | yes      | —                              | Mnemonic of the funding account used to top up accounts on nightly/production test contours |
| `REFERRAL_WEB_HOST`         | `feature/become-citizen/impl` | no       | `referral.example.com`         | Host of the web app that backs referral (`https`) deeplinks                 |
| `GAME_RESULTS_FALLBACK_URL` | `feature/videogame/impl`      | no       | `https://example.com/`         | Last-resort URL for the game-results webview (after DotNs + Remote Config)   |

> `NIGHTLY_FUNDING_MNEMONIC` controls a funding account — keep it in a secret store
> or untracked `local.properties`, never commit it. The build refuses to configure
> without it, so a fork that does not use the top-up flow still has to supply a
> throwaway mnemonic (`python3 scripts/generate-mnemonic.py` produces one). The
> separate testnet Alice origin continues to use the public, well-known Substrate
> development fixture.
>
> The value is compiled into `BuildConfig` and can therefore be recovered from a
> distributed APK. GitHub Secrets protect it at rest and mask it in CI logs, but do
> not make it confidential after compilation. Use only a tightly funded disposable
> test account, never a treasury or other valuable mnemonic.

### 5.3 Sentry (crash/error reporting)

| Variable             | Used by               | Required | Description                                            |
|----------------------|-----------------------|----------|--------------------------------------------------------|
| `SENTRY_ORG`         | `app/build.gradle.kts`| yes      | Sentry org slug handed to the Gradle plugin. Mandatory at configuration time even when you do not use Sentry — any slug works then. |
| `SENTRY_PROJECT`     | `app/build.gradle.kts`| yes      | Sentry project slug handed to the Gradle plugin. Same rule as `SENTRY_ORG`. |
| `SENTRY_AUTH_TOKEN`  | Sentry Gradle plugin  | no       | Token for source upload. Only used for `debug`/`nightly` (release variant is ignored). Omit to skip upload. |
| `SENTRY_DSN`         | `app` debug/nightly manifests (`${sentryDsn}` manifest placeholder) | no | DSN crashes/errors are reported to. Default: empty (Sentry reporting disabled). Only the `debug`/`nightly` manifests reference it; `release` has no DSN meta-data. |

Point `SENTRY_ORG` / `SENTRY_PROJECT` at **your** Sentry and set `SENTRY_DSN` to the
project's DSN; while `SENTRY_DSN` is unset, crash reporting stays disabled.

### 5.4 Build

| Variable      | Used by                                   | Required | Description                                   |
|---------------|-------------------------------------------|----------|-----------------------------------------------|
| `CI_BUILD_ID` | `build-logic/convention/src/main/kotlin/Versions.kt` (`versionCode`)| no | Integer `versionCode`. Defaults to `28` if unset. |

The marketing `versionName` is stored in `build-logic/convention/src/main/kotlin/Versions.kt`
(`DefaultVersionName`) and managed by the scripts in §6.

### 5.5 Legacy / not currently consumed by the build

The previous CI fetched the following secrets, but **no source or build file in this
repository currently reads them**. They are listed for completeness; treat them as
optional and only wire them up if you re-introduce the corresponding integration:

`RAISE_CLIENT_ID_DEBUG`, `RAISE_SECRET_DEBUG`, `RAISE_CLIENT_ID_PROD`,
`RAISE_SECRET_PROD`, `MERCURYO_PRODUCTION_SECRET`, `POSTHOG_HOST`,
`POSTHOG_API_KEY`, `POSTHOG_API_KEY_DEV`, `POSTHOG_API_KEY_PROD`.

### 5.6 Publishing/infra secrets (channel-dependent — see §7)

These are **not** consumed by the Gradle build; you only need them in the publishing
step of whatever channel you choose:

| Variable                          | Channel                       | Description                                |
|-----------------------------------|-------------------------------|--------------------------------------------|
| `FIREBASE_GOOGLE_SERVICE_ACCOUNT` | Firebase App Distribution     | Service-account JSON (content)             |
| Firebase **App ID**               | Firebase App Distribution     | Target Firebase app id                     |
| Play service-account JSON         | Google Play                   | For Play Developer API uploads             |
| `GITHUB_TOKEN`                    | GitHub Releases               | Provided automatically in GitHub Actions   |

---

## 6. Building

Distribution editions: `gp` (Google services — Google Play) and `vanilla` (no GMS —
GrapheneOS / sideload). Build types: `debug`, `nightly`, `safetynet`, `release`.
`safetynet` is a `nightly` derivative with the safety-mode flags on (`SAFETY_MODE`,
`TAB_BAR_CONNECTIVITY_INDICATOR` in `common/build.gradle.kts`); it reuses the nightly
manifest and the dev signing config.

Build types also pick the on-chain environment and the compile-time feature flags.
Both are `buildConfigField`s in `common/build.gradle.kts`, consumed through
`common/src/main/java/io/paritytech/polkadotapp/common/utils/FeatureFlags.kt`; a few
developer-only options (`DEBUG_MENU`, `SKIP_MOBRULE_CASE`, …) follow `BuildConfig.DEBUG`
instead. Change them there if your fork needs a different cut.

| Field | `debug` | `nightly` | `safetynet` | `release` | Effect |
|-------|---------|-----------|-------------|-----------|--------|
| `TESTNET_ENVIRONMENT` | `TESTNET` | `NIGHTLY` | `NIGHTLY` | `PRODUCTION` | Chain environment. Only `PRODUCTION` reads the `chains` Remote Config key; the others read `chains_v2` (§4.1). Also enables the faucet top-up outside `PRODUCTION`. |
| `SAFETY_MODE` | off | off | on | on | On hides the full feature set: arbitrary `.dot` products, browse tab, full tab bar, all chat extensions, linked devices, product settings, personhood, collectibles, ID-card rank. |
| `TAB_BAR_CONNECTIVITY_INDICATOR` | off | off | on | on | Chain-health indicators in the tab bar. |
| `COINAGE_DEBUG_FEATURES` | on | on | on | off | "Debug features" card under the balance card (breakdown, faucet top-up, log sharing). |
| `TESTNET_FUND_ENABLED` | on | on | on | off | Log-sharing action on the Coinage card. |
| `DIM1_ENABLED` | on | on | on | off | DIM1 game entry points in the Peer bot and the weekly-game footer. |
| `FAQ_ENABLED` | on | on | on | off | FAQ entries in the Peer and tattoo bots. |
| `ALLOW_SHORT_EVIDENCE_VIDEO` | on | off | off | off | Accept short evidence videos in Proof-of-Ink. |
| `PEER_BOT_BY_DEFAULT`, `DIM1_BOT_BY_DEFAULT`, `SAMPLE_BOT` | on | off | off | off | Bot chats pre-created for a new account. |
| `DIM2_BOT_BY_DEFAULT` | on | on | off | off | Same, for the DIM2 bot. |

### 6.1 `gp` edition (Google Play)

Standard builds for devices with Google Play Services. Requires `google-services.json`
at `app/google-services.json` (§4) and all secrets in §5.

```bash
# Debug (dev signing)
./gradlew assembleGpDebug

# Nightly (dev signing)
./gradlew assembleGpNightly

# Release (release signing) — requires the release keystore + secrets
./gradlew assembleGpRelease
```

Outputs land in `app/build/outputs/apk/gp/<buildType>/`.

### 6.2 `vanilla` edition (GrapheneOS / GMS-free)

The `vanilla` edition produces an APK with **no Google Play Services dependency**: no
Play Integrity, no GMS-backed Sign-In, no Analytics or Crashlytics. It is designed to
run correctly on [GrapheneOS](https://grapheneos.org/) and any other Android
distribution that does not ship Google Play Services. Firebase Remote Config is the
one Firebase service it keeps, because the app cannot function without it; it needs no
Play Services on the device.

Prerequisites that differ from the `gp` edition:

- **`google-services.json` still required** — see §4; Remote Config needs it.
- **`GOOGLE_OAUTH_ID` / `GOOGLE_PROJECT_ID` unused at runtime** — `tools/auth/impl`
  uses a no-op sign-in and `tools/integrity/impl` compiles `0` for `vanilla`. They are
  still read at configuration time (§5.2), so set them to any non-empty value.
- All other variables (signing, Sentry, app values) apply identically.

```bash
# Debug (dev signing)
./gradlew assembleVanillaDebug

# Nightly (dev signing)
./gradlew assembleVanillaNightly

# Release (release signing) — requires the release keystore + secrets
./gradlew assembleVanillaRelease
```

Outputs land in `app/build/outputs/apk/vanilla/<buildType>/`.

The release APK (`app-vanilla-release.apk`) is the artifact distributed to
GrapheneOS users (see §7 → *GrapheneOS / direct APK sideload*).

### Version management

The marketing version and the build number are defined in
`build-logic/convention/src/main/kotlin/Versions.kt`:

```kotlin
private const val DefaultVersionName = "1.0.0"   // versionName
private const val DefaultVersionCode = 28        // versionCode (fallback)
```

- **versionName** — edit `DefaultVersionName` directly when cutting a release.
- **versionCode** — edit `DefaultVersionCode`, or override per build (without editing
  the file) via the `CI_BUILD_ID` environment variable (see §5.4), which takes
  precedence when set.

---

## 7. Publishing channels (pick what you need)

The workflows under `.github/workflows` implement the maintainers' own variants of
some of these channels (Firebase App Distribution, nightly APKs to object storage);
they are tied to that infrastructure and are not meant to run unchanged in a fork.
Below is **how** each channel is typically wired so you can implement it on your own
infrastructure.

### Google Play

- Upload `app-gp-release.aab`/`.apk` to the Play Console manually, **or**
- Use the Play Developer API (e.g. the `r0adkll/upload-google-play` GitHub Action or
  `fastlane supply`) with a Play service-account JSON.

### Firebase App Distribution (internal/QA builds)

- Upload the signed APK with a Firebase service account
  (`FIREBASE_GOOGLE_SERVICE_ACCOUNT`) and the target **App ID**, e.g. via the
  Firebase CLI (`firebase appdistribution:distribute`) or an equivalent GitHub Action.

### GitHub Releases

Build both editions first, then create a single release attaching both APKs:

```bash
gh release create "v$VERSION" \
  --title "$VERSION" \
  --notes "release notes" \
  app/build/outputs/apk/gp/release/app-gp-release.apk \
  app/build/outputs/apk/vanilla/release/app-vanilla-release.apk
```

`gh` uses the automatically-provided `GITHUB_TOKEN` in Actions. Users on Google Play
receive the `gp` APK through the store; GrapheneOS users download and sideload
`app-vanilla-release.apk` directly (via the Files app, `adb install`, or the
[GrapheneOS App Store](https://github.com/GrapheneOS/Apps) as a longer-term channel).

### Object storage / direct APK

- Upload the APK to any bucket/CDN (S3-compatible, GCS, etc.) and publish the link.

---

## 8. GitHub Actions

The workflows under `.github/workflows` use the configuration described above.
For a fork or another deployment, store non-sensitive branding and endpoint
configuration as **GitHub Actions variables**, and credentials, signing material,
and mnemonics as **GitHub Actions secrets**. Expose both as environment variables;
the Gradle build reads them through the helpers in §5, so no code changes are needed.
A minimal sketch follows the table.

The nightly release notification also reads two workflow-only repository variables:

| Variable | Description |
|----------|-------------|
| `CI_MATRIX_ROOM_IDS` | Comma-separated Matrix room IDs that receive the notification. |
| `NIGHTLY_DOWNLOAD_LINKS` | Multiline Markdown list passed to the notification action as its download links. |

```yaml
name: Build
on: [pull_request]
jobs:
  build:
    runs-on: ubuntu-latest
    env:
      APPLICATION_ID: ${{ vars.APPLICATION_ID }}
      APPLICATION_NAME: ${{ vars.APPLICATION_NAME }}
      DEBUG_APPLICATION_NAME: ${{ vars.DEBUG_APPLICATION_NAME }}
      NIGHTLY_APPLICATION_NAME: ${{ vars.NIGHTLY_APPLICATION_NAME }}
      PRIVACY_POLICY_URL: ${{ vars.PRIVACY_POLICY_URL }}
      TERMS_OF_USE_URL: ${{ vars.TERMS_OF_USE_URL }}
      LOG_COLLECTION_EMAIL: ${{ vars.LOG_COLLECTION_EMAIL }}
      CONTACT_EMAIL: ${{ vars.CONTACT_EMAIL }}
      CURRENCY_SYMBOL: ${{ vars.CURRENCY_SYMBOL }}
      FIAT_SYMBOL: ${{ vars.FIAT_SYMBOL }}
      REFERRAL_WEB_HOST: ${{ vars.REFERRAL_WEB_HOST }}
      GAME_RESULTS_FALLBACK_URL: ${{ vars.GAME_RESULTS_FALLBACK_URL }}
      SENTRY_ORG: ${{ vars.SENTRY_ORG }}
      SENTRY_PROJECT: ${{ vars.SENTRY_PROJECT }}
      SENTRY_DSN: ${{ vars.SENTRY_DSN }}
      FIRESTORE_DATABASE_ID: ${{ secrets.FIRESTORE_DATABASE_ID }}
      GOOGLE_OAUTH_ID: ${{ secrets.GOOGLE_OAUTH_ID }}
      GOOGLE_PROJECT_ID: ${{ secrets.GOOGLE_PROJECT_ID }}
      CI_KEYSTORE_PASS: ${{ secrets.CI_KEYSTORE_PASS }}
      CI_KEYSTORE_KEY_ALIAS: ${{ secrets.CI_KEYSTORE_KEY_ALIAS }}
      CI_KEYSTORE_KEY_PASS: ${{ secrets.CI_KEYSTORE_KEY_PASS }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-java@v4
        with: { distribution: temurin, java-version: "21" }
      # ... install Android SDK, NDK r29, Rust + targets, Clang, Node, Python (see §2)
      - run: |
          echo "${{ secrets.DEV_KEYSTORE_BASE64 }}" | base64 --decode > develop_key.jks
          # both editions — Remote Config requires google-services.json
          echo "${{ secrets.GOOGLE_SERVICES_JSON_BASE64 }}" | base64 --decode > app/google-services.json
      - name: Build
        env:
          NIGHTLY_FUNDING_MNEMONIC: ${{ secrets.NIGHTLY_FUNDING_MNEMONIC }}
        run: ./gradlew assembleGpDebug --no-daemon --stacktrace
      # The vanilla edition needs the same variables and files; only the task differs:
      # - run: ./gradlew assembleVanillaDebug --no-daemon --stacktrace
```

Add a publishing job (§7) only on the events/branches you want to ship from.

---

## 9. Localization

All UI strings live in a single module: `common/src/main/res/values/strings.xml`
holds the default (English) resources, and no other module declares strings.

The app has no language list of its own. Users pick a language in the Android 13+
system per-app language screen, which the app's *Settings → Language* row opens.
Android 12 and older show no such screen and follow the device language. Web products
receive the same locale through the `localeSubscribe` host call, so they need no
extra wiring.

Adding a language takes one resource file and one registration.

### 9.1 Add the translated strings

Create `common/src/main/res/values-<qualifier>/strings.xml`, where `<qualifier>` is the
Android resource qualifier for the language: `values-fr`, `values-pt-rBR`,
`values-b+zh+Hant`. Translate the `<string>` and `<plurals>` entries of the default
file, keeping the same names. Skip the ones marked `translatable="false"`. Any key
missing from the translation falls back to English at runtime.

The build sets no `localeFilters` / `resConfigs`, so every `values-*` folder ships in
the APK without Gradle changes.

### 9.2 Register the locale in `locale-config`

`app/src/main/res/xml/locales_config.xml` is referenced from the manifest
(`android:localeConfig`). The system language screen lists exactly the locales in this
file. It is maintained by hand (the build does not enable `generateLocaleConfig`), so
add a `<locale>` element with the BCP-47 tag matching the qualifier from §9.1 (`fr`,
`pt-BR`, `zh-Hant`):

```xml
<locale-config xmlns:android="http://schemas.android.com/apk/res/android">
    <locale android:name="en" />
    <locale android:name="fr" />
</locale-config>
```

A translation without this entry still applies on devices whose system language
matches it. It just can't be picked per app.

### 9.3 Verify

Build any variant (§6) and install it on an Android 13+ device. Open *Settings →
Language* in the app, check that the new language is listed, select it, and check that
the UI switches to the translation.
