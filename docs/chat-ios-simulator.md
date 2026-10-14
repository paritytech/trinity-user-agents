# Chat Extension v2 in the iOS simulator

This runbook runs the Chat Extension v2 sample product (`playground/chat-v2`) in the iPhone simulator app, through the
real Rust core, and writes the diagnosis rows of the test plan for the iOS layer.

```bash
make ios-chat-v2-run IOS_HOST=hosts/ios \
  IOS_SWIFT_FLAGS='-DF_DEV -DUNSTABLE -DTESTNET_FEATURE -DFEATURE_PRODUCTS -DFEATURE_SIGN_IN -DFEATURE_INPUT -DTRUAPI_RUNTIME_DEFAULT' \
  IOS_GOOGLE_SERVICE_PLIST=hosts/ios/polkadot-app/GoogleService-Info.plist \
  IOS_SIMULATOR_DEVICE=<udid>
```

`IOS_SWIFT_FLAGS` replaces the app's `BASE_SWIFT_FLAGS`. The default of the Makefile names the flags of the separate
`polkadot-app-ios-v2` checkout, which do not turn on `FEATURE_PRODUCTS` in `hosts/ios`; the value above is the Debug set
of `hosts/ios/Configs/base.debug.xcconfig`. To run against an app that is already built, call the launcher directly:

```bash
TRUAPI_IOS_E2E_DEVICE=<udid> TRUAPI_IOS_E2E_APP=<path>/polkadot-app.app node scripts/launch-ios-chat-v2.mjs
```

## Prerequisites

- The TrUAPI Swift package built for the simulator: `npm ci --ignore-scripts`,
  `TRUAPI_SKIP_PACKAGE_BUILD=1 ./scripts/codegen.sh`, `SIM_ONLY=1 ./ios/truapi-host/scripts/rebuild.sh`.
- `hosts/ios/polkadot-app/Generated/Secrets.generated.swift` and a **real** Firebase `GoogleService-Info.plist`. The app
  reads its chains, DotNS addresses and services from Firebase Remote Config. With the placeholder plist that
  `Runscripts/setup-secrets.sh` writes, the app stops at "Configuration is broken" before any product runs; the launcher
  detects this in the console and stops with that reason.
- A simulator. The launcher needs no onboarding: the simulator-only hook `TRUAPI_IOS_E2E_SEED_IDENTITY` selects the
  theme, creates a random wallet and stores the lite username `truapie2e.00` when the simulator has no wallet. The
  username is not registered on chain and is marked as not claimed.

## What the launcher does

1. Builds the sample product with `CHAT_V2_DOTNS_NAME=truapi-chat-v2.dot`.
2. Boots the simulator and installs the app.
3. Installs `out/worker/index.js` as the chat worker of `truapi-chat-v2.dot` in the app's product storage. A short
   prefix makes `console.info` (the event log of the sample) reach the app's console bridge, and writes each `Error` as
   its name and message.
4. Starts the app with `simctl launch --console-pty` and keeps its console in `app.log`. The simulator-only hooks inject
   the product as a chat bot (`TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST`), open its `host` room (`TRUAPI_IOS_E2E_OPEN_CHAT`) and
   start the command feed (`TRUAPI_IOS_E2E_CHAT_COMMANDS`).
5. Waits until the product has created its `host` room in Core Data.
6. Sends the commands of each scenario through the command feed: one JSON file `{"room", "text"}` in the app's
   `tmp/truapi-e2e/commands`, which the app sends to the product as a user message and marks `.done` or `.failed`. The
   launcher then waits for the `MessagePosted` line of the event log and for the expected effect, and takes a
   screenshot.
7. Writes `playground/test-results/ios-chat-v2/diagnosis-report.md`, `event-log.jsonl`, `app.log` and the screenshots.

## Rows

The report has one row for each scenario of the test plan that names the iOS layer (04-TESTING section 4).

| Kind       | Scenarios                                                                                              | Row                                                                                                |
| ---------- | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------- |
| Driven     | S2 (`create_room` V2, name and icon), S1 (`/post legacy`), S34 (`/footer empty`, then `/footer input`) | `pass` or `fail` with the first failed check                                                       |
| Observed   | S4 (`/preview pin`), S7 (`/post text`)                                                                 | `skip` with the refusal the app gave: the app does not serve `setRoomPreview` and `updateFeed` yet |
| Not driven | every other iOS scenario                                                                               | `skip` with the phase that adds it (P3 to P7)                                                      |

A row moves from `skip` to driven when its phase lands in the app. The launcher fails when a driven row fails or has no
row.
