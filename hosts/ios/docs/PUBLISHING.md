# Building, configuring & publishing

This guide explains how to configure the app, sign it, and distribute it to
**TestFlight** and **Firebase App Distribution**.

> This repository ships a **default CI/CD setup**: GitHub Actions workflows
> (`.github/workflows/`) driven by Fastlane lanes (`fastlane/`) — see §10. It is
> preconfigured for the upstream project, so a fork must supply its own secrets,
> `match` signing repo, and runners before it runs green. Sections 5–9 document
> the underlying Apple/Google tooling the pipeline automates, so you can also
> build, sign and distribute by hand or port the flow to another CI system
> (GitLab CI, Bitrise, Xcode Cloud, …).

---

## 1. How configuration works

Configuration comes from three places:

- **Brand identity** — bundle id, display name, deep-link scheme, universal-link
  domains, legal URLs — in `Configs/brand.xcconfig` (§4).
- **Build-time secrets** (Sentry DSN, Meld token) — environment variables baked
  into the app by `generate_secrets.sh` (§2).
- **Runtime configuration** — the chain set, backend URLs, contract addresses —
  fetched at launch from Firebase Remote Config (§3). Nothing in this group is
  bundled with the app: without a Firebase project serving these keys the app
  stays on its startup screen.

The build-time secrets are wired through three files:

| File | Committed? | Purpose |
|------|-----------|---------|
| `Runscripts/generate_secrets.sh` | ✅ yes | Generates the Swift below from `env-vars.sh` / the environment. |
| `polkadot-app/env-vars.sh` | ❌ gitignored | Your secrets. Created from `env-vars.template.sh`. |
| `polkadot-app/Generated/Secrets.generated.swift` | ❌ gitignored | Generated Swift (`enum GeneratedSecrets`) read by the app. Produced by `generate_secrets.sh`. |

Resolution order (highest priority first): **`env-vars.sh` → the process
environment**. Any value left unset becomes an empty string, which simply
disables the corresponding feature. The app therefore builds and runs with safe
public defaults even with no secrets configured.

The **"Generate Secrets"** Xcode build phase runs `generate_secrets.sh`
automatically on every build (it is skipped when `RUN_IN_CI=true` — see the CI
note below), so you rarely need to run it by hand.

### First-time local setup

```bash
./Runscripts/setup-secrets.sh
```

This scaffolds `env-vars.sh` and the `GoogleService-Info` plists from their
`*.template` files (without overwriting anything that already exists) and
generates `Secrets.generated.swift`. The app then builds; features that need a
real secret stay disabled until you provide one. Reaching the UI additionally
needs the Remote Config parameters from §3.

> **In CI:** set `RUN_IN_CI=true` (which skips the in-Xcode generation and the
> Google-plist copy), export the variables directly into the job environment
> from your secret store, and run `./Runscripts/generate_secrets.sh` before
> building. Provide the active `GoogleService-Info.plist` yourself.

---

## 2. Environment variables

### Secrets — set these in `polkadot-app/env-vars.sh` or the CI environment

| Variable | Used for | If empty |
|----------|----------|----------|
| `SENTRY_DSN` | Sentry crash/issue reporting DSN (ignored unless the build links Sentry — see §9) | Issue monitoring disabled |
| `MELD_BASIC_AUTH_TOKEN` | Meld fiat on-ramp basic auth (`<key>:<secret>`, base64) | Fiat on-ramp auth unset |

### Signing & distribution — GitHub Actions secrets (not needed for local simulator runs)

These are the names the shipped pipeline reads (`.github/workflows/`,
`.github/actions/`). Unlike the table above, they **are** contracts: rename one
and the workflow that reads it fails.

Required for any signed build:

| Secret | Used for | Read by |
|--------|----------|---------|
| `ASC_KEY_ID` | App Store Connect API key ID | TestFlight upload, build-number lookup, device registration, signing refresh |
| `ASC_ISSUER_ID` | App Store Connect API issuer ID | same as above |
| `ASC_KEY_BASE64` | Base64-encoded `.p8` private key | same as above |
| `KEYCHAIN_PASSWORD` | Password for the temporary CI keychain | every signed build |
| `MATCH_PASSWORD` | Passphrase that decrypts the `match` assets | every signed build |
| `FASTLANE_RO_PAT` | Fine-grained PAT with read access to the `match` repo | every signed build |
| `FASTLANE_RW_PAT` | The same PAT with write access | `ios-update-signing-data.yml` only |
| `GOOGLE_SERVICE_INFO_DEV_BASE64` | Base64 development `GoogleService-Info.plist` | PR builds and tests |
| `GOOGLE_SERVICE_INFO_RELEASE_BASE64` | Base64 production `GoogleService-Info.plist` | release and nightly archives, nightly simulator build |
| `GOOGLE_SERVICE_INFO_SAFETY_BASE64` | Base64 Safetynet `GoogleService-Info.plist` (separate Firebase app for `…​.safety`) | Safetynet archives |

Required only by the distribution target you actually use:

| Secret | Used for | Read by |
|--------|----------|---------|
| `CREDENTIAL_FILE_CONTENT` | Google service-account JSON for App Distribution | `ios-firebase-debug-distribution.yml` |
| `FIREBASE_APP_ID` | Firebase App Distribution app ID (`1:…:ios:…`) | `ios-firebase-debug-distribution.yml` |
| `SENTRY_AUTH_TOKEN` | Uploading dSYMs to Sentry (the build phase skips when `sentry-cli` is unconfigured — see §9) | signed builds |

Optional — these gate reporting steps only, and a fork can leave them unset:

| Secret | Used for | Read by |
|--------|----------|---------|
| `NOTIFICATION_BOT_URL`, `NOTIFICATION_BOT_TOKEN` | The relay the announcements post through | `ios-nightly-distribution.yml`, `ios-release-distribution.yml` |
| `CI_MATRIX_ROOM_IDS` | Comma separated rooms to announce into. One message per room | `ios-nightly-distribution.yml`, `ios-release-distribution.yml` |
| `TESTFLIGHT_DISTRIBUTION_LINK` | One ready-to-render markdown link entry, e.g. `[TestFlight](https://testflight.apple.com/join/<id>)`. Kept in a secret so the access hint stays out of this public repository | `ios-nightly-distribution.yml`, `ios-release-distribution.yml` |

`SENTRY_DSN` and `MELD_BASIC_AUTH_TOKEN` from the first table are also stored as
GitHub Actions secrets, because CI runs `generate_secrets.sh` from
`.github/actions/configure-secrets` instead of reading `env-vars.sh`.

Tester groups are a workflow variable rather than a secret: `FIREBASE_GROUPS` is
set in `ios-firebase-debug-distribution.yml` (default `polkadotapp-ios`) and can be
overridden per run.

Backend and on-chain endpoints (identity backend, IPFS gateway, DotNS
contracts, game dashboard) are **not** build-time variables: the app fetches
them at runtime via Firebase Remote Config — see §3.

---

## 3. Firebase setup

The app uses Firebase for Remote Config. The real `GoogleService-Info.plist`
files are **not** committed.

1. Create a Firebase project and register two iOS apps — one for development
   (bundle id `…​.develop`) and one for production.
2. Download each `GoogleService-Info.plist` and save them as:
   - `polkadot-app/GoogleService/GoogleService-Info-Dev.plist`
   - `polkadot-app/GoogleService/GoogleService-Info-Release.plist`
   - `polkadot-app/GoogleService/GoogleService-Info-Safety.plist`
3. During a build, the **"Google info"** build phase copies the correct one to
   `polkadot-app/GoogleService-Info.plist` based on `$CONFIGURATION`
   (Debug/Dev/DevCI → Dev, Safetynet → Safety, Release/Nightly → Release). In CI
   (`RUN_IN_CI=true`) this copy is skipped — provide the active plist yourself.

`*.plist.template` files document the expected structure with placeholder values;
`setup-secrets.sh` copies them into the real filenames so a fresh checkout builds
with an inert Firebase configuration until you drop in real plists.

### Remote Config parameters

At launch the app fetches Remote Config (`FirebaseApplicationService`) and waits
for the chains below to come up before showing any UI. If the fetch fails or the
chain set is missing, it stays on the startup screen with a "Configuring
application…" notice.

Required for the app to start:

| Parameter | Type | Value |
|-----------|------|-------|
| `chains_v2` | JSON array | The chain set. Decoded as `[RemoteChainModel]` (`Packages/ChainRegistry/Sources/ChainRegistry/Model/RemoteChain/`): each entry carries `chainId`, `name`, `addressPrefix`, `assets` (`assetId`, `symbol`, `precision`, …), `nodes` (`url`, `name`) and optional `genesisHash`, `options`, `explorers`, `types`, `additional`. The `chainId` values and asset indices are fixed by the build configuration — see below. |
| `identity_backend_url` | string | Base URL of the identity backend. The backend is open source: [device-uniqueness-backend-community](https://github.com/paritytech/device-uniqueness-backend-community). |
| `ipfs_gateway_url` | string | IPFS gateway used to fetch DotNS-published dApp content. |
| `dot_ns_config` | JSON object | `{"resolverContractAddress": "<hex>", "registryContractAddress": "<hex>"}` — DotNS contracts on the Asset Hub chain (`pallet-revive`). `registryContractAddress` may be omitted or empty, which disables manifest resolution. |
| `coinage_instance_id` | string | Decimal `UInt32` — the Coinage instance the app pays through. |
| `game_dashboard_url` | string | DIM2 game dashboard base URL. Required only in builds compiled with `TESTNET_FEATURE`; ignored otherwise. |

Optional — an absent key disables or degrades the feature it drives:

| Parameter | Type | Used for |
|-----------|------|----------|
| `latest_ios_version` | string | Latest published app version; logged only. |
| `funding_config` | JSON object | `{"onrampUrl": "getcash.dot", "offrampUrl": "https://getcash.dot/offramp"}` — the dApp destinations the CASH card opens for top up and withdraw, each a dot-domain or a full URL. Without them the CASH card entry points report unavailable. |
| `funding_domain` | string | Legacy DotNS label of the funding dApp; read only when `funding_config.onrampUrl` is absent. |
| `cross_chain_transfers`, `xcm_general_config` | JSON | XCM transfer routes for the deposit flow (`XcmTransfersSyncService`). Deposits via XCM stay unavailable without them. |
| `transaction_extension_versions` | JSON object | `{"<chainId>": <uint8>}` — transaction-extension version per chain; defaults to `0`. |
| `collectibles_enabled` | bool | Shows the collectibles entry on the wallet screen. |
| `payment_asset_config` | JSON object | `{"symbol": "CASH", "iconSquareUrl": "https://…/square.svg", "iconWideUrl": "https://…/wide.svg"}` — the payment asset's symbol and logos: a square mark for amounts and payment messages, a wide mark-plus-wordmark for the balance card, as absolute web URLs (SVG or PNG; an SVG must not set `fill="none"` on its root element, which the iOS renderer cannot draw). Each field is optional; anything missing or failing to load falls back to the bundled brand (`BRAND_CASH_SYMBOL` and the built-in mark). Logos are fetched as soon as the config is applied and kept cached by URL, so a change shows on the next config refresh without an app update; publish a changed logo under a new URL. Test assets live in `docs/assets/payment-asset/`. |
| `collectibles_fallback_url` | string | Web URL used when the collectibles dApp cannot be resolved through DotNS. |
| `game_results_fallback_url` | string | Web URL used when the game-results dApp cannot be resolved through DotNS. |
| `app_sharing_url` | string | Download link included in the message the ID card's Share button composes ("Download it at {link} and add me – my username is {username}."). Without it the message is shared without the link sentence. |

**Chain ids per environment.** `KnownChainId` (`polkadot-app/AppConfig/KnownChains.swift`)
hardcodes the `chainId` strings the app looks up in `chains_v2`, and
`SupportedAssets` the `assetId` indices it expects inside those chains. Which
set a build uses is decided by its environment compiler flag (§4 shows which
configuration sets which flag). Your chain set must use the same identifiers:

| Environment flag | People chain | Asset Hub | Bulletin |
|------------------|--------------|-----------|----------|
| `UNSTABLE` | `preview-people` | `preview-ah` | `preview-bulletin` |
| `NIGHTLY` | `nightly-people` | `nightly-ah` | `nightly-bulletin` |
| neither (release) | `release-people` | `release-ah` | `release-bulletin` |

Asset Hub entries must expose the native asset at index `0`, USDT at `1`, USDC at
`2`, the funded stable asset at `3` (not used in `UNSTABLE`) and PGAS at `4`; the
People chain exposes the native asset at `0` and the app's main asset at `1`
(`65` in `UNSTABLE`).

---

## 4. Build configurations

| Configuration | Bundle id | Environment flag | Environment |
|---------------|-----------|------------------|-------------|
| `Debug` / `DevCI` | `…​.develop` | `UNSTABLE` | Unstable preview backend |
| `Nightly` | production id | `NIGHTLY` | Stable testnet, full feature set — TestFlight internal testers |
| `Release` | production id | — | Mainnet — TestFlight internal testers |
| `Safetynet` | `…​.safety` | `NIGHTLY` | Nightly chains, Release feature set — the release build proven before release; separate app, installs alongside production |

Brand identity lives in `Configs/brand.xcconfig`, included by every
`Configs/base.*.xcconfig`. Change it to your own identifiers before signing or
distributing — the bundle id, App Group, keychain access group and universal-link
domains of every build derive from it. `Configs/brand.template.xcconfig`
documents the key contract:

| Key | Used for |
|-----|----------|
| `BRAND_DISPLAY_NAME` | App name; some configurations append a suffix (`Configs/base.*.xcconfig`) |
| `BRAND_BUNDLE_ROOT` | Bundle id root; per-configuration suffixes and the `NotificationServiceExtension` id derive from it |
| `BRAND_DEEPLINK` | Custom URL scheme base; per-configuration suffixes are appended |
| `BRAND_SHARE_ROOT`, `BRAND_SHARE_ROOT_TEST` | Universal-link (`applinks:`) domains for production and test builds — host only, no scheme |
| `BRAND_CASH_SYMBOL`, `BRAND_FIAT_SYMBOL` | Currency symbols shown in the UI |
| `BRAND_TERMS_HOST`, `BRAND_PRIVACY_HOST` | Terms of use / privacy policy pages — host and path, no scheme |
| `BRAND_CONTACT_EMAIL` | Support contact shown in the app |

Every key must be present and non-empty: a missing one expands to an empty
string and the app traps at launch naming the key (`AppConfig.Brand`). Per-
configuration values (compiler flags, icon suffix, bundle suffix) stay in
`Configs/base.*.xcconfig`, `polkadot-app/Configs/*.xcconfig` and
`NotificationServiceExtension/Configs/*.xcconfig`.

---

## 5. Code signing

You need an Apple Developer account and a registered App ID for the app **and**
its `NotificationServiceExtension`.

Recommended: **App Store Connect API key** (`.p8`) for non-interactive signing
and uploads. Generate one in App Store Connect → Users and Access → Integrations
→ Keys. The shipped pipeline reads it from the `ASC_KEY_ID`, `ASC_ISSUER_ID`
and `ASC_KEY_BASE64` secrets (§2); if you drive the upload yourself, export it
under whatever names your own tooling expects.

For certificates and provisioning profiles, pick one of:

- **Xcode automatic signing** — simplest for local builds and Xcode Cloud.
- **A shared signing repo** (e.g. Fastlane `match`) — store certs/profiles
  encrypted in a private git repo; each machine/CI runner fetches them. `match`
  works with any private repo you control.
- **Manual** — export a Distribution certificate (`.p12`) and the provisioning
  profiles, import them into the build keychain in CI.

In CI, create a dedicated keychain, import the certificate, and select the right
provisioning profile via the export options when archiving.

---

## 6. Build & archive

```bash
# Generate build-time config (also run automatically by the Xcode build phase)
./Runscripts/generate_secrets.sh

# Archive
xcodebuild -project polkadot-app.xcodeproj \
  -scheme polkadot-app \
  -configuration Release \
  -archivePath build/polkadot-app.xcarchive \
  archive

# Export a signed .ipa (provide an ExportOptions.plist describing the method,
# team id and provisioning profiles)
xcodebuild -exportArchive \
  -archivePath build/polkadot-app.xcarchive \
  -exportOptionsPlist ExportOptions.plist \
  -exportPath build/
```

A minimal `ExportOptions.plist` for App Store distribution:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>method</key><string>app-store</string>
  <key>teamID</key><string>YOUR_TEAM_ID</string>
  <key>uploadSymbols</key><true/>
</dict></plist>
```

Use `method = ad-hoc` (or `development`) for Firebase App Distribution builds.

---

## 7. Distribute to TestFlight

After exporting an App Store `.ipa`, upload it with Apple's notarised tool using
your App Store Connect API key:

```bash
xcrun altool --upload-app -f build/polkadot-app.ipa -t ios \
  --apiKey "$ASC_KEY_ID" \
  --apiIssuer "$ASC_ISSUER_ID"
```

(`xcrun altool` reads the `.p8` from `~/.appstoreconnect/private_keys/` or
`./private_keys/`. `xcrun notarytool`/`Transporter` are alternatives.)

The build then appears in App Store Connect → TestFlight. Add it to internal or
external tester groups there, or automate group assignment with the App Store
Connect API.

---

## 8. Distribute to Firebase App Distribution

Build an `ad-hoc` signed `.ipa`, then upload with the Firebase CLI:

```bash
firebase appdistribution:distribute build/polkadot-app.ipa \
  --app "$FIREBASE_APP_ID" \
  --groups "$FIREBASE_GROUPS" \
  --release-notes "Your release notes"
```

Authenticate the CLI with a Google service account that has the **Firebase App
Distribution Admin** role (`GOOGLE_APPLICATION_CREDENTIALS` pointing at the
service-account JSON, or `--service-credentials-file`).

---

## 9. Crash symbols (Sentry)

Sentry is **linked only when the build environment sets `ISSUE_MONITORING=sentry`**.
`Packages/IssueMonitoring/Package.swift` reads it at resolve time, so without it
sentry-cocoa never enters the dependency graph.

Fastlane sets the variable for every configuration except `Release`, caches
resolved packages per flavour, and `verify_no_issue_monitoring` fails the build on
any Sentry symbol or DSN found in the archive. Local `Debug` builds and the nightly
simulator build also run without Sentry, since neither sets the variable.

The **"Upload Debug Symbols to Sentry"** Xcode build phase uploads dSYMs on all
configurations except `Debug` and `Release`, and only when `sentry-cli` is
installed; otherwise it prints a warning and continues. The `distribute_testflight`
lane skips its `sentry_debug_files_upload` for `Release` on the same grounds.

> The build phase currently hardcodes `SENTRY_ORG` and `SENTRY_PROJECT` (set to
> the upstream project). **Change these to your own org/project** — or remove
> the build phase entirely — before uploading symbols from your own builds.
> Authenticate `sentry-cli` with a `SENTRY_AUTH_TOKEN` in your environment.

---

## 10. The default CI/CD setup

The repo ships a GitHub Actions pipeline (`.github/workflows/`) driven by Fastlane
lanes (`fastlane/`): PR build and tests, plus TestFlight and Firebase
distribution. Distribution workflows are manual (`workflow_dispatch`) and gated to
named maintainers. The pipeline is preconfigured for the upstream project — a fork
supplies its own GitHub Actions secrets, `match` signing repo, and runners to run
it. The steps above (§5–9) are what the lanes automate.

The secrets the workflows expect are listed in §2 under "Signing & distribution",
grouped by whether they are required for any signed build, required for a
particular distribution target, or optional. A fork also needs to repoint
`fastlane/Matchfile` and `.github/actions/install` at its own `match` repository,
and `fastlane/Appfile` at its own Apple team.

Keep every credential in the GitHub Actions secret store — never commit
`env-vars.sh`, the real `GoogleService-Info` plists, signing certificates, or API
keys.
