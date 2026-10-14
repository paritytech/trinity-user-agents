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
   the product as a chat bot (`TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST`), hold its worker and start the command feed
   (`TRUAPI_IOS_E2E_CHAT_COMMANDS`), and skip the notification prompt (`TRUAPI_IOS_E2E_NO_PUSH_PROMPT`).
5. Waits until the product has created its `host` room in Core Data and its bot has started, then opens the `host` room
   through the room link pipeline (`polkadot://<product>/-/chat/host`), so the room screen holds the worker and sends
   `RoomOpened`.
6. Goes through the phases of each scenario. A command goes to the product through the command feed: one JSON file
   `{"room", "text"}` in the app's `tmp/truapi-e2e/commands`, which the app sends to the product as a user message and
   marks `.done` or `.failed`. An action of the user goes to the same feed as `{"room", "host", "argument", "target"}`:
   the simulator-only harness (`TrUAPIE2EHostAction`, `ChatProductRoomPresenter+E2E.swift`) runs the code path of the
   gesture on the room screen that shows the room (`open` through the room link pipeline, `list`, `send`, `menu`,
   `confirm`, `toolbar`, `participants`, `participant`, `reactions`, `suggestion`, `type`, `attach`, `button`,
   `footer-action`, `older`, `jump`, `retry`, `retry-room`, `back`, `bottom`). The launcher then waits for each
   expectation: Core Data rows, the event log of the product, or the `[e2e:ui] <key> <room> <value>` console markers
   that the room screen writes when what it shows changes (`window`, `readiness`, `toolbar`, `menu`, `footer`, `header`,
   `author-<id>`, `sheet`, `reactions`, `suggestions`, `catalogue`, `activity`, `placeholder`, `allowed`, `failure`). It
   takes a screenshot at the end of each scenario and where a phase asks for one.
7. Writes `playground/test-results/ios-chat-v2/diagnosis-report.md`, `event-log.jsonl`, `app.log` and the screenshots.

`TRUAPI_IOS_E2E_ONLY=S3,S25` drives the named scenarios only. The app keeps its data between runs, so a scenario checks
what changed in the run and not absolute counts.

## Rows

The report has one row for each scenario of the test plan that names the iOS layer (04-TESTING section 4). The steps and
the checks of each row are in `scripts/lib/ios-chat-v2-scenarios.mjs`.

| Kind       | Scenarios                                                                                                                                                                | Row                                          |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------- |
| Driven     | S1 to S4, S7 to S22, S24 to S41, S46                                                                                                                                     | `pass` or `fail` with the first failed check |
| Not driven | S5 (no command calls `delete_room`), S6 (the harness holds the worker), S23 (an uncaught error does not stop an iOS worker), S44 (no command exceeds the rate), S47 (P7) | `skip` with the reason                       |

The launcher fails when a driven row fails or has no row.
