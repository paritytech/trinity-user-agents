# host-playground in the native hosts

Runs the public `host-playground` product inside the iOS and Android host apps and reports how each of its tests ended.

- `tests.json` names the product, the host-playground commit the suite was written against, and the tests to run in order.
- `page-runner.js` is injected into the product page by each driver and runs one test per call.
- `report.mjs` turns a run's `results.json` into `report.md` and a one-line summary.

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
