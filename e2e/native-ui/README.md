# Native UI flows

[Maestro](https://maestro.dev) flows that drive the iOS and Android host apps through their own screens. One flow serves both apps: it finds elements by the iOS accessibility identifiers in `hosts/ios/Packages/ExternalAccessibility` and by Android test tags with the same names, which the app exposes as resource ids.

- `smoke.yaml`: starts the signed-in app, finds the account's username on the Pocket tab, opens the cash card's Send screen and backs out, opens Settings, and returns to Chats.
- `flows/`: steps the smoke flow shares, such as opening a tab. iOS tucks its docklet away after a few idle seconds, so opening a tab there swipes it back in first. The e2e Android build keeps its docklet on screen.
- `run.mjs`: runs `smoke.yaml` and writes a JUnit report, Maestro's debug output and `report.md` to `--out`.

`.github/workflows/host-playground-e2e.yml` runs them after host-playground, against the same app and account, so they run whenever that workflow does. The username they look for is the one the e2e account holds, set next to each platform's step.

## Running locally

Install the Maestro CLI, sign in to the app on a simulator or emulator, then:

```bash
node e2e/native-ui/run.mjs --platform ios --app-id io.parity.polkadotapp \
  --username <the account's username> --out target/native-ui [--device <udid>]
```

A nightly build signed in by hand works. Its Android docklet hides itself, so turn off Settings > Auto-hide docklet first. `maestro hierarchy` prints the ids on screen while a flow is being written.
