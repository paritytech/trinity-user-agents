# Building the apps for development

The iOS app builds in CI or locally for the simulator. The Android app builds in CI. Nothing built either way can be published: every path to testers needs a credential that only CI holds.

To request the dev credentials a local iOS build needs, reach out to the host SDK team.

## What builds where

| | Local | CI |
|---|---|---|
| iOS simulator | Yes, with the dev credentials | `ios-simulator-build` label |
| iPhone | | `ios-device-build` label, once your device is registered |
| Android emulator or phone | | `android-device-build` label |
| TestFlight, Firebase App Distribution, Play Store, GitHub releases | | Release workflows only |

Every build here talks to the dev backend, so its accounts and backups land there.

## Builds from a pull request

Add the label to your pull request. The run posts a comment on it with the download link and install commands, and every later push rebuilds while the label is set.

| Label | Runs on | Delivers | Kept for |
|---|---|---|---|
| `ios-simulator-build` | macOS | An arm64 simulator `.app`, for Apple Silicon Macs | 14 days |
| `ios-device-build` | macOS | A signed `.ipa` for registered devices | 14 days |
| `android-device-build` | Linux | An APK signed with the shared develop key | 5 days |

The Android build does not queue behind the macOS jobs.

The jobs read repository secrets, which a pull request from a fork does not get, so they need a branch in this repository.

### Android emulator

The APK onboards on an emulator only when the emulator has Google Play: Play Integrity attests the device before a username is claimed, and a `google_apis` image without the Play Store fails with "Google Play services aren't available on this device". An API 35 Play Store image works:

```bash
sdkmanager "system-images;android-35;google_apis_playstore;arm64-v8a"
avdmanager create avd -n play35 -k "system-images;android-35;google_apis_playstore;arm64-v8a" -d pixel_6
emulator -avd play35
```

Sign the emulator into a Google account before onboarding; claiming a username opens Google sign-in.

## iOS simulator, locally

Prerequisites: Xcode 26.4.1, the version CI pins (`hosts/ios/.github/actions/install/action.yml`), and the Rust toolchain with `aarch64-apple-ios-sim`.

| File | Contents | Required |
|---|---|---|
| `hosts/ios/polkadot-app/GoogleService/GoogleService-Info-Dev.plist` | Firebase config for `io.parity.polkadotapp.develop`, from the host SDK team | Yes |
| `hosts/ios/polkadot-app/env-vars.sh` | `SENTRY_DSN`, `MELD_BASIC_AUTH_TOKEN` | No; empty values turn crash reporting and the fiat on-ramp off |

Both are gitignored (`hosts/ios/.gitignore`).

```bash
hosts/ios/Runscripts/setup-secrets.sh       # scaffolds env-vars.sh and the plists
# then replace hosts/ios/polkadot-app/GoogleService/GoogleService-Info-Dev.plist with the dev plist
xcrun simctl boot "iPhone 17 Pro"
make ios-sim-run                            # IOS_SIM_DEVICE=<udid> picks a simulator other than the booted one
```

`make ios-sim-run` builds with `make ios-sim-build`, then installs and launches the app. The first run installs the npm dependencies and runs `SIM_ONLY=1 make ios-bootstrap` to generate the core. After that a cold app build takes about 3 minutes on an M-series Mac and an incremental one under a minute. Rerun `SIM_ONLY=1 make ios-bootstrap` after changing the `truapi` or `truapi-provider` crates. Opening `hosts/ios/polkadot-app.xcodeproj` in Xcode after the bootstrap works the same way.

**The dev plist is required to launch.** The target refuses to build with the template, because the template `setup-secrets.sh` copies in carries `GOOGLE_APP_ID = 1:YOUR_GCM_SENDER_ID:ios:YOUR_APP_ID`, which `FirebaseApp.configure()` (`polkadot-app/Common/Firebase/FirebaseApplicationService.swift:43`) rejects with an uncaught exception, and a well-formed fake id fails next in Firebase Installations. `hosts/ios/docs/PUBLISHING.md` §1 describes the templates as inert; on a simulator they crash.

**Ad hoc signing is required.** The target builds with `CODE_SIGN_IDENTITY="-"`, which embeds the App Group and keychain entitlements without a provisioning profile; an unsigned build (`CODE_SIGNING_ALLOWED=NO`) launches without them and fails when the app opens its App Group container. These are the flags CI's simulator build uses (`hosts/ios/fastlane/lanes/lane_simulator_build.rb:71-73`).

**Debug is the configuration to build.** It uses the `.develop` bundle id that the dev plist is registered for, and the simulator's attestation path, so onboarding completes without a device.

**Onboard on a fresh simulator.** The app keeps its account in the simulator's keychain, which survives uninstalling the app, so a simulator that has run any build of `io.parity.polkadotapp.develop` opens straight into that old account. Create a new one with `xcrun simctl create "Local build" "iPhone 17 Pro"`, or erase one with `xcrun simctl erase <udid>`.

## What never leaves CI

| Credential | What it would allow |
|---|---|
| match password, distribution certificate | Signing iOS builds as the team |
| App Store Connect key | Uploading to TestFlight |
| Develop keystore | An APK that installs over testers' apps |
| Release keystore | Play Store releases |
| Firebase App Distribution credential | Delivering to tester groups; issued only to runs on `main` |
| Remote Config write access | Changing what every tester's app loads |

## Open questions

- **Fixing the iOS templates.** Either the templates carry a well-formed inert configuration that boots, or `PUBLISHING.md` stops calling them inert. The fix belongs upstream, since `hosts/ios` is vendored.
