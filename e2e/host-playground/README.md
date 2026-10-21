# host-playground in the native hosts

Runs the public `host-playground` product inside the iOS and Android host apps and reports how each test ended.

- `tests.json`: the product, the host-playground commit the list matches, the per-test timeout, the tests that leave the product and where they land, the tests in order, and `knownFailures`, tests expected to fail with a given message for a reason outside the hosts.
- `page-runner.js`: injected into the product page; runs one test per call.
- `report.mjs`: turns `results.json` into `report.md` and a one-line summary.
- `release-notes.mjs`: adds a run's results to a nightly prerelease's notes.
- `android/`, `ios/`: each platform's runner and the code added to its app for these builds only, so `hosts/` is never changed.

`.github/workflows/host-playground-e2e.yml` runs in three ways: from each nightly, on the commit it built, adding the results to that nightly's prerelease without ever failing the night; on a pull request labelled `host-playground-e2e`; and by dispatch. Each runner writes `results.json`, `report.md` and diagnostics for failures to `--out`, and exits non-zero when a test failed or the run stopped early.

A manual dispatch defaults to Android. Its Android-only `native-telemetry` mode builds an exact Host commit, opens the requested DotNS product, and writes a sanitized correlation result without raw Sentry envelopes, screenshots, or logcat. The uniquely marked acceptance event is captured by the document-start tap; this mode returns a synthetic success to keep the fast local acceptance isolated from Sentry ingestion.

`native-telemetry-ingestion` uses the same device scenario but passes Sentry requests through. It resolves the Android public DSN from the native project's active client key, or checks the optional `SENTRY_DSN_ANDROID` secret against that project, without printing the DSN. The Sentry token is scoped to preflight and backend query steps; grant it `org:read` for project discovery and Explore queries plus `project:read` for client-key lookup and event details. Defaults are the EU API (`https://de.sentry.io`), the `paritytech` organization, the `polkadot-android` native project, and the `jollity-next` product project. Optional `SENTRY_ORG`, `SENTRY_NATIVE_PROJECT`, and `SENTRY_JOLLITY_PROJECT` variables can override these defaults when set to real slugs. The backend gate polls for up to 20 rounds at 15-second intervals by default (configurable with `SENTRY_POLL_ATTEMPTS` and `SENTRY_POLL_INTERVAL_MS`), queries by numeric project IDs and captured session UUID, and fetches the exact marked Jollity event by its envelope event ID. It checks the native event's exact message, Host session, variant, Android platform, app version, TruAPI version, and finite numeric build and monotonic-time context extras. It also checks the Jollity event's session tag, native-host context, and `session.link` breadcrumb, plus the `previous-session-end` log's build and version fields. The backend artifact contains sanitized project IDs, event IDs, timestamps, check stages, and the generated UUID; partial stage evidence is written before failures.

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
