# Building the apps locally

The iOS app runs on the simulator and the Android app on the emulator from a local build, with the dev Firebase configuration and a signing key of your own. Nothing built this way can be published: every path to testers needs a credential that only CI holds.

## What a local build can and cannot do

| | Local build | Needs CI |
|---|---|---|
| iOS simulator | Yes, ad hoc signed, no provisioning profile | |
| Android emulator or phone (sideloaded) | Yes, signed with your own dev key | |
| iPhone | | `ios-device-build`, once your device is registered |
| Replace a tester's installed app | | The develop keystore |
| TestFlight, Firebase App Distribution, Play Store, GitHub releases | | The App Store Connect key, match, the Firebase credential, the release keystore |

Remote Config is read only from a local build. A local build talks to the same dev backend as the CI debug builds, so its accounts and backups land there.

## Configuration

Every file below is gitignored (`hosts/ios/.gitignore`, `hosts/android/.gitignore`), so none of it can be committed.

### iOS

| File | Contents | Required |
|---|---|---|
| `hosts/ios/polkadot-app/GoogleService/GoogleService-Info-Dev.plist` | Firebase config for `io.parity.polkadotapp.develop` | Yes |
| `hosts/ios/polkadot-app/env-vars.sh` | `SENTRY_DSN`, `MELD_BASIC_AUTH_TOKEN` | No; empty values turn crash reporting and the fiat on-ramp off |

The plist is required to launch, not only to load data. The template `setup-secrets.sh` copies in carries `GOOGLE_APP_ID = 1:YOUR_GCM_SENDER_ID:ios:YOUR_APP_ID`, which `FirebaseApp.configure()` (`polkadot-app/Common/Firebase/FirebaseApplicationService.swift:43`) rejects with an uncaught exception, and a well-formed fake id fails next in Firebase Installations. `hosts/ios/docs/PUBLISHING.md` §1 describes the templates as inert; on a simulator they crash.

### Android

| File | Contents | Required |
|---|---|---|
| `hosts/android/app/google-services.json` | Firebase config whose client list includes `<APPLICATION_ID>.debug` | Yes |
| `hosts/android/local.properties` | `sdk.dir`, `ndk.dir`, and the values below | Yes |
| `hosts/android/develop_key.jks` | A dev keystore you generate | Yes, every `assemble*` task signs |

`local.properties` values the build refuses to configure without (`hosts/android/docs/DEPLOYMENT.md` §5):

- **App identity:** `APPLICATION_ID`, `APPLICATION_NAME`, `IOS_BUNDLE_ID`
- **Links:** `PRIVACY_POLICY_URL`, `TERMS_OF_USE_URL`, `CONTACT_EMAIL`, `LOG_COLLECTION_EMAIL`
- **Display:** `CURRENCY_SYMBOL`, `FIAT_SYMBOL`
- **Google:** `GOOGLE_OAUTH_ID`, `GOOGLE_PROJECT_ID`, `FIRESTORE_DATABASE_ID`
- **Sentry:** `SENTRY_ORG`, `SENTRY_PROJECT`
- **Funding:** `NIGHTLY_FUNDING_MNEMONIC`, any throwaway mnemonic; the top-up flow is the only reader
- **Signing:** `CI_KEYSTORE_PASS`, `CI_KEYSTORE_KEY_ALIAS`, `CI_KEYSTORE_KEY_PASS`, for the keystore you generate

Two of these come from `google-services.json` itself: `GOOGLE_PROJECT_ID` is its `project_info.project_number` with an `L` suffix (`README.md:642`), and `GOOGLE_OAUTH_ID` is the `client_id` of its `client_type: 3` (web) OAuth client. Keep the dev project's number: the app hands it to Play Integrity, and onboarding attestation fails with a project the app is not linked to. `FIRESTORE_DATABASE_ID` only selects where cloud backups go, and `(default)` builds and runs. The remaining values are this repository's Actions variables, readable with `gh api repos/paritytech/trinity-user-agents/actions/variables`; `FIAT_SYMBOL` is `$` and `IOS_BUNDLE_ID` is `io.parity.polkadotapp`.

With placeholder values the app still runs as far as the onboarding screens, on local Remote Config defaults. The dev `google-services.json` is what makes it fetch Remote Config and connect to the `preview-*` chains. That file only needs the `.debug` client for a debug build; a `safetynet` build also needs `<APPLICATION_ID>.safetynet`.

## iOS simulator

Prerequisites: Xcode 26.4.1, the version CI pins (`hosts/ios/.github/actions/install/action.yml`), and the Rust toolchain with `aarch64-apple-ios-sim`.

```bash
npm ci --ignore-scripts
SIM_ONLY=1 make ios-bootstrap               # about 2 minutes
hosts/ios/Runscripts/setup-secrets.sh       # scaffolds env-vars.sh and the plists
# then replace hosts/ios/polkadot-app/GoogleService/GoogleService-Info-Dev.plist with the dev plist

cd hosts/ios
TRUAPI_USE_LOCAL_BINARY=1 xcodebuild build \
  -project polkadot-app.xcodeproj -scheme polkadot-app -configuration Debug \
  -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath build_simulator_dd -clonedSourcePackagesDirPath source_packages \
  -skipPackagePluginValidation -skipMacroValidation ARCHS=arm64 ONLY_ACTIVE_ARCH=NO \
  CODE_SIGN_IDENTITY="-" CODE_SIGNING_REQUIRED=NO CODE_SIGNING_ALLOWED=YES

xcrun simctl install booted build_simulator_dd/Build/Products/Debug-iphonesimulator/polkadot-app.app
xcrun simctl launch booted io.parity.polkadotapp.develop
```

A cold build takes about 3 minutes on an M-series Mac and an incremental one under a minute. Opening `hosts/ios/polkadot-app.xcodeproj` in Xcode after `make ios-bootstrap` works the same way.

**Ad hoc signing is required.** `CODE_SIGN_IDENTITY="-"` embeds the App Group and keychain entitlements without a provisioning profile; an unsigned build (`CODE_SIGNING_ALLOWED=NO`) launches without them and fails when the app opens its App Group container. These are the flags CI's simulator build uses (`hosts/ios/fastlane/lanes/lane_simulator_build.rb:71-73`).

**Debug is the configuration to build.** It uses the `.develop` bundle id that the dev plist is registered for, and the simulator's attestation path, so onboarding completes without a device.

Rerun `make ios-bootstrap` after changing the `truapi` or `truapi-provider` crates.

## Android emulator

Prerequisites (`hosts/android/README.md`): JDK 21, the Android SDK with compileSdk 36 and NDK r29, Rust with the four Android targets and `cargo-ndk`, Node.js, and Python 3.

```bash
cd hosts/android
keytool -genkeypair -keystore develop_key.jks -alias local-dev \
  -keyalg RSA -keysize 2048 -validity 10000      # once; the passwords go in local.properties
# write local.properties and copy in app/google-services.json
./gradlew assembleGpDebug                        # about 7 minutes cold, about 3 incremental
adb install -r app/build/outputs/apk/gp/debug/app-gp-debug.apk
adb shell am start -n io.parity.polkadotapp.debug/io.paritytech.polkadotapp.app.root.presentation.root.RootActivity
```

**Your keystore is your own.** The `dev` signing config reads `DEV_KEYSTORE_FILE`, defaulting to `hosts/android/develop_key.jks` (`hosts/android/app/build.gradle.kts:47`). Android refuses to update an app signed with a different key, so a build signed with yours can never replace a tester's app, and a CI build of `io.parity.polkadotapp.debug` has to be uninstalled before yours installs.

**Build `gp`, not `vanilla`.** `vanilla` replaces Google sign-in, Firebase auth, push and backup with stubs, so it cannot sign in.

## What never leaves CI

| Credential | What it would allow |
|---|---|
| match password, distribution certificate | Signing iOS builds as the team |
| App Store Connect key | Uploading to TestFlight |
| Develop keystore | An APK that installs over testers' apps |
| Release keystore | Play Store releases |
| Firebase App Distribution credential | Delivering to tester groups; issued only to runs on `main` |
| Remote Config write access | Changing what every tester's app loads |

The configuration in this guide is not secret. A Firebase file ships inside every build of the app it configures, and the Android build values are compiled into the nightly APK, which is a public download. It is shared through the team vault rather than chat so the copy everyone uses is the current one.

## Open questions

- **Where the dev configuration lives.** Proposed: one vault entry holding the dev plist, the dev `google-services.json` and the shared `local.properties` values, readable by everyone with write access to this repository.
- **Fixing the iOS templates.** Either the templates carry a well-formed inert configuration that boots, or `PUBLISHING.md` stops calling them inert. The fix belongs upstream, since `hosts/ios` is vendored.
