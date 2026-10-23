# Building the iOS and Android apps

The apps under `hosts/ios` and `hosts/android` are not built on a pull request by default. App builds are slow, and the iOS ones take macOS runners, which CI has few of. A label asks for a build. Main, the nightlies and the develop distributions build on their own.

## On a pull request

Add a label to get a build. It builds the head commit, and rebuilds on every push while the label stays on. Remove the label to stop. The first time a pull request touches an app, a bot comment lists the labels that build it.

| Label | What you get | How to use it |
| --- | --- | --- |
| `ios-simulator-build` | A DevCI compile, the unit tests, and a simulator `.app` | Install on an Apple Silicon Mac simulator, see below |
| `ios-device-build` | A signed ad hoc archive | Installs on registered devices only |
| `android-device-build` | A signed APK | Installs on any phone with `adb install` |
| `android-instrumented-tests` | The app's connected tests on an emulator | Reports only, never blocks a merge |

The three builds post a comment on the pull request linking the download, updated in place as the branch moves. iOS builds are kept for 14 days, the Android APK for 5.

Simulator build:

```bash
gh run download <run-id> --name simulator-preview-<short-sha>
unzip polkadot-app-*.app.zip
xcrun simctl install booted polkadot-app.app
xcrun simctl launch booted io.parity.polkadotapp.develop
```

iOS device build: to add a phone, reach out to the host SDK team with its name and UDID and we will register it for you. Push again afterwards so the next archive includes it.

Android build:

```bash
gh run download <run-id> --name android-preview-pr-<number>
adb install -r polkadot-app-<short-sha>.apk
```

Labels need write access. Device builds also need the signing secrets, which a pull request from a fork cannot reach, so push the branch to this repository to get one.

Without any label, CI still compiles the TrUAPIHost Swift package and the Kotlin host adapter against the core. That catches binding drift, but it does not compile either app.

## After merge

| Workflow | When | Delivers |
| --- | --- | --- |
| `ios-pr.yml` | Every push to main that touches the app or its core | The simulator build and tests, as on a labelled pull request |
| `ios-firebase-debug-distribution.yml` | Every merged pull request | A DevCI ad hoc build to the iOS develop testers on Firebase App Distribution |
| `android-debug-distribution.yml` | Every push to main | A debug APK to the Android develop testers on Firebase App Distribution |

## Nightlies

| | iOS | Android |
| --- | --- | --- |
| Workflow | `ios-nightly-prepare.yml` | `android-nightly.yml` |
| Starts | 20:00 UTC | 22:00 UTC |
| Delivers | TestFlight, to the "Daily test builds" group | Firebase App Distribution, to the daily test group |
| Also published | A simulator bundle on a `simulator-*` GitHub prerelease | `app-gp-nightly.apk` and `app-vanilla-nightly.apk` on the `nightly-android` release |

Both skip a scheduled night when main has not moved since their last successful build, and both can be run by hand from the Actions tab. Each announcement lists the pull requests the build carries.

The latest Android nightly always downloads from the same link:

```
https://github.com/paritytech/trinity-user-agents/releases/download/nightly-android/app-gp-nightly.apk
```

GitHub prereleases are public, so anything built into a nightly is public.

## Building locally

iOS: run `make ios-bootstrap` once, then open `hosts/ios/polkadot-app.xcodeproj`. Rerun it after changing the `truapi` or `truapi-provider` crates. `SIM_ONLY=1` skips the device slice, which is enough for the simulator.

Android: set the variables listed in `hosts/android/README.md` in `hosts/android/local.properties`, then:

```bash
cd hosts/android
./gradlew assembleGpDebug
```

The README covers each workflow in more depth.
