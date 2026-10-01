# host-playground end to end

Runs the public `host-playground` product inside a host app and records how
each of its tests ends. `tests.json` names the product, the host-playground
commit the list was taken from and the ordered test ids. `page-runner.js` runs
one test inside the page, and `report.mjs` turns a run's `results.json` into
`report.md` plus a one-line summary.

## iOS

The iOS host runs the list itself. A simulator build with the `E2E_TEST`
compilation condition carries `HostPlaygroundE2E` (in
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

Build the app with the simulator lane, which is the only place `E2E_TEST` is
set. It needs the in-tree core bootstrapped first (`make ios-bootstrap`), a
`GoogleService-Info.plist` for the app's bundle id and the generated secrets
file, as in `.github/workflows/ios-nightly-simulator-release.yml`:

```bash
( cd hosts/ios && bundle exec fastlane build_app_simulator )
```

Then run the list. The seed phrase is read from a file and never printed:

```bash
make e2e-host-playground-ios HOST_PLAYGROUND_IOS_MNEMONIC_FILE=/path/to/seed
# or
node e2e/host-playground/ios/run.mjs \
  --app hosts/ios/build_simulator/polkadot-app.app \
  --mnemonic-file /path/to/seed \
  --out artifacts/host-playground-ios \
  [--device <udid>] [--timeout-minutes 120]
```

`--app` also takes the `polkadot-app-simulator.app.zip` the nightly publishes.
Without `--device` the runner picks a simulator the way the other iOS scripts
do: `TRUAPI_IOS_E2E_DEVICE`, then a device named for TrUAPI E2E, then a booted
iPhone. It exits non-zero when the run did not finish or any test failed.
