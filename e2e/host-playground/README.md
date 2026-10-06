# host-playground in the native hosts

Runs the public `host-playground` product inside the iOS and Android host apps and reports how each of its tests ended.

- `tests.json` names the product, the host-playground commit the suite was written against, and the tests to run in order.
- `page-runner.js` is injected into the product page by each driver and runs one test per call.
- `report.mjs` turns a run's `results.json` into `report.md` and a one-line summary.

`.github/workflows/host-playground-e2e.yml` runs both platforms in CI, on pull requests that touch this directory and by manual dispatch, signing in with the `E2E_ANDROID_MNEMONIC` and `E2E_IOS_MNEMONIC` test accounts and uploading each run's results and report.

## Android

The Android driver runs a nightly build of `hosts/android` that carries the hooks in `android/hooks`. They open the app's WebViews to DevTools and, when the runner leaves a mnemonic at `files/e2e-seed` in the app's data directory, consume the file and restore that account and its on-chain username. The hooks are added by a Gradle init script, so `hosts/android` itself is unchanged:

```bash
cd hosts/android
./gradlew --init-script ../../e2e/host-playground/android/e2e.init.gradle.kts :app:assembleGpNightly
```

The build reads the same app config as any other `hosts/android` build (`APPLICATION_ID`, `APPLICATION_NAME` and the rest, from `local.properties` or the environment) and signs with the dev keystore (`DEV_KEYSTORE_FILE`, `CI_KEYSTORE_PASS`, `CI_KEYSTORE_KEY_ALIAS`, `CI_KEYSTORE_KEY_PASS`). The gp flavour needs `app/google-services.json`, and it has to be the real project's: the app reads its chain list from Firebase Remote Config, and without chains the account restore waits forever on the people chain for the network's dotNS TLD. The hooks give each seeding step two minutes and then report which one stalled.

With an emulator or device attached:

```bash
node e2e/host-playground/android/run.mjs \
  --apk hosts/android/app/build/outputs/apk/gp/nightly/app-gp-nightly.apk \
  --mnemonic-file <file holding the test account's mnemonic> \
  --out target/e2e-host-playground/android \
  [--serial <adb serial>]
```

`make e2e-host-playground-android MNEMONIC_FILE=<path>` does both steps. The account must already hold a username, since the app only counts an account with one as onboarded.

The runner reinstalls the app, grants its runtime permissions, copies the mnemonic into the app's data directory through `run-as` and waits for the `HostPlaygroundE2E` logcat line that reports the outcome. It then relaunches the app, opens `polkadotapp://host-playground.paseo` and attaches to the product's WebView page over the DevTools protocol. It needs only Node 22 or later and `adb`, found through `ADB`, `ANDROID_HOME` or `PATH`. While the tests run it taps the native approval sheets they raise; the accepted labels are `APPROVE_LABELS` in `android/run.mjs`. The package is `${APPLICATION_ID}.nightly`, with `APPLICATION_ID` defaulting to `io.parity.polkadotapp`.

`--out` receives `results.json` and `report.md`, a screenshot for each failed test, and, when anything failed, `logcat.txt` holding only the app's own lines with long hex strings and addresses masked. The runner exits 0 when nothing failed, 1 when a test failed and 2 when the run could not start.

## iOS

The iOS host runs the list itself. A simulator build with the
`HOST_PLAYGROUND_E2E` compilation condition carries `HostPlaygroundE2E` (in
`hosts/ios/polkadot-app/Modules/Products/TrUAPI/`), which stays inert unless
the app is launched with `TRUAPI_IOS_E2E_HOST_PLAYGROUND=1`. `ios/run.mjs`
installs the app fresh, places the seed phrase, `tests.json` and
`page-runner.js` in the app's `tmp/truapi-e2e/`, and launches it. The app then:

1. reads and deletes the seed, restores the wallet from it and marks the theme
   chosen, before the root gates decide, so launch lands on the regular
   username check, which reads the account's on-chain username;
2. waits for the main tab bar, resolves `host-playground` against the current
   dotNS top-level domain and opens it;
3. injects `page-runner.js` into the product's web view and runs each test in
   order, injecting again whenever the page has reloaded and opening the
   product again when a test navigated away from it;
4. answers the native signing sheet and permission prompts while a test is
   pending, by tapping the approving control (the labels live in
   `HostPlaygroundE2EApprover`);
5. rewrites `results.json` after every test and writes `done` at the end, or
   `failure` with the reason when setup could not finish.

The runner waits for `done`, brings the app back to the front when a test sent
it to Safari, copies `results.json` to the output directory and runs
`report.mjs`. When the run fails it also saves a screenshot and the hook's own
log lines (`io.parity.polkadotapp.e2e`), which carry no account data.

The account must already have an on-chain username on the network the build
targets: an account without one stops at the username claim screen.

Build the app with the simulator lane and `HOST_PLAYGROUND_E2E=1`, which adds
the condition; the nightly's published simulator build leaves it out. It needs the in-tree core bootstrapped first (`make ios-bootstrap`), a
`GoogleService-Info.plist` for the app's bundle id and the generated secrets
file, as in `.github/workflows/ios-nightly-simulator-release.yml`:

```bash
( cd hosts/ios && HOST_PLAYGROUND_E2E=1 bundle exec fastlane build_app_simulator )
```

Then run the list. The seed phrase is read from a file and never printed:

```bash
make e2e-host-playground-ios MNEMONIC_FILE=/path/to/seed
# or
node e2e/host-playground/ios/run.mjs \
  --app hosts/ios/build_simulator/polkadot-app.app \
  --mnemonic-file /path/to/seed \
  --out target/e2e-host-playground/ios \
  [--device <udid>] [--timeout-minutes 120]
```

`--app` also takes a zipped `.app`.
Without `--device` the runner picks a simulator the way the other iOS scripts
do: `TRUAPI_IOS_E2E_DEVICE`, then a device named for TrUAPI E2E, then a booted
iPhone. It exits non-zero when the run did not finish or any test failed.
