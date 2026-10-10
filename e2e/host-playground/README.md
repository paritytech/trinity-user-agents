# host-playground in the native hosts

Runs the public `host-playground` product inside the iOS and Android host apps and reports how each test ended.

- `tests.json`: the product, the host-playground commit the list matches, the per-test timeout, the tests that leave the product and where they land, the tests in order, and `knownFailures`, tests expected to fail with a given message for a reason outside the hosts.
- `page-runner.js`: injected into the product page; runs one test per call.
- `report.mjs`: turns `results.json` into `report.md` and a one-line summary.
- `android/`, `ios/`: each platform's runner and the code added to its app for these builds only, so `hosts/` is never changed.

`.github/workflows/host-playground-e2e.yml` runs both platforms daily on main, on a pull request labelled `host-playground-e2e`, and by dispatch. Each runner writes `results.json`, `report.md` and diagnostics for failures to `--out`, and exits non-zero when a test failed or the run stopped early.

The test account must already hold an on-chain username on Paseo. Its mnemonic is read from a file and never printed.

## Android

Needs the app config any `hosts/android` build reads, the dev keystore, and the real project's `app/google-services.json`, since the app reads its chain list from Remote Config. Then, with an emulator or device attached:

```bash
make e2e-host-playground-android MNEMONIC_FILE=<path> [ANDROID_SERIAL=<serial>]
```

## iOS

Needs the in-tree core bootstrapped (`make ios-bootstrap`), a `GoogleService-Info.plist` for the bundle id and the generated secrets file, as in `.github/workflows/ios-nightly-simulator-release.yml`. Then:

```bash
e2e/host-playground/ios/build.sh
make e2e-host-playground-ios MNEMONIC_FILE=<path> [IOS_SIMULATOR_DEVICE=<udid>]
```
