# Chat Extension v2 in the Android emulator

This runbook runs the Chat Extension v2 sample product (`playground/chat-v2`) in the Android app inside a headless
emulator, through the real Rust core, and writes the diagnosis rows of the test plan for the Android layer. It is the
twin of the [iOS simulator runbook](chat-ios-simulator.md).

```bash
make android-chat-v2-run ANDROID_GOOGLE_SERVICES_JSON=~/Documents/google-services.json
```

The target copies the Firebase config into `hosts/android/app/google-services.json` (gitignored), builds the vanilla
debug APK with `hosts/android/gradlew :app:assembleVanillaDebug`, and starts `scripts/launch-android-chat-v2.mjs`. To
run against an APK that is already built, call the launcher directly:

```bash
node scripts/launch-android-chat-v2.mjs
```

## Prerequisites

- JDK 21, the Android SDK on `ANDROID_HOME` with `platform-tools`, `emulator`, `build-tools` (for `aapt2`), NDK
  `29.0.14206865`, and the Rust target `aarch64-linux-android` (or `x86_64-linux-android` on an Intel host).
- A gitignored `hosts/android/local.properties` with `sdk.dir`, the placeholder secrets that `app/build.gradle.kts`
  reads, `rust.targets=arm64`, `rust.pythonCommand=python3`, and a debug keystore: `DEV_KEYSTORE_FILE`,
  `CI_KEYSTORE_PASS`, `CI_KEYSTORE_KEY_ALIAS`, `CI_KEYSTORE_KEY_PASS` (any keystore that `keytool -genkeypair` makes).
- A **real** `google-services.json` that lists the debug package (`<APPLICATION_ID>.debug`). The app reads its chains
  and services from Firebase Remote Config.
- The JS client built (`npm ci --ignore-scripts`, `TRUAPI_SKIP_PACKAGE_BUILD=1 ./scripts/codegen.sh`,
  `npm run build --prefix js/packages/truapi`) and `yarn install --frozen-lockfile` in `playground/chat-v2`.
- An arm64 (or x86_64) API 36 AVD named `truapi-chat-e2e`. To keep it apart from your own AVDs, put it in a folder of
  its own and export that folder for the run:

  ```bash
  export ANDROID_AVD_HOME=$PWD/.android-avd
  avdmanager create avd -n truapi-chat-e2e -k "system-images;android-36;google_apis;arm64-v8a" -d pixel_5
  ```

  The image must be `google_apis` (not `google_apis_playstore`): S23 kills the WebView renderer with `su`.

The launcher boots the AVD headless when no device is online and stops it at the end; a device that was online before
the run stays up. `TRUAPI_ANDROID_E2E_DEVICE` picks a serial, `TRUAPI_ANDROID_E2E_KEEP_EMULATOR=1` keeps the emulator.

## What the launcher does

1. Builds the sample product with `CHAT_V2_DOTNS_NAME=truapi-chat-v2.dot`, serves `playground/chat-v2/out` at
   `http://127.0.0.1:3100`, and maps the port into the emulator with `adb reverse`.
2. Installs the APK (the package comes from `aapt2`, or `TRUAPI_ANDROID_E2E_PACKAGE`), clears its data, turns the
   on-screen keyboards off (and closes one that the system turns on again before each screenshot), and records the whole
   logcat into `app.log`.
3. Starts the app and sends the debug-only `E2EHookReceiver` broadcast: it seeds a wallet and a lite username (no
   onboarding taps) and installs `truapi-chat-v2.dot` as a chat product with the served worker. Then it restarts the app
   and waits until the product has created its `host` and `source` rooms in `chat_rooms`, and until the worker logs its
   preimage line (the end of its start; the launcher answers the preimage prompt with "Don't allow"). When either does
   not come in time, the launcher restarts the app once and notes it in the report.
4. Runs each scenario as a user: it opens a room with the chat deeplink `polkadotapp://chat?chatId=<hex>`, types the
   commands of the sample product into the room input (the `clipboard` hook puts the text on the clipboard and the
   launcher pastes it), taps buttons and menu entries that it finds with `uiautomator dump`, and scrolls with swipes.
   The S4 commands of the `host` room go through the `message` hook while the chat list is on screen.
5. Proves each row from the event log of the worker (its `console.info` lines, which the app logs under the tag
   `ProductWebChromeClientKt`), the app database (`run-as <package> sqlite3 -json databases/app_v2.db`), the screen and
   a screenshot.
6. Writes `playground/test-results/android-chat-v2/diagnosis-report.md` (gitignored) with `event-log.jsonl`, `app.log`
   and the screenshots `sNN-<step>.png`.

Before each step in a room the launcher waits until the worker runs and its client is connected: a room that opens soon
after another one closed restarts the worker. When the room opens as unavailable, the launcher taps Retry and adds a
line to "Observed while driving the app" in the report.

## Rows

The report has one row for each scenario of the test plan that names the Android layer (04-TESTING section 4), in the
row format of the iOS report.

| Kind       | Scenarios                                                                        | Row                                           |
| ---------- | -------------------------------------------------------------------------------- | --------------------------------------------- |
| Driven     | S1 to S4 and S6 to S14 (`host` room), S11 and S15 to S24 and S46 (`source` room) | `pass`, or `fail` with the first failed check |
| Not driven | S5 (the sample has no command that calls `delete_room`)                          | `skip` with that reason                       |
| Not driven | S25 to S41                                                                       | `skip: needs P8.6`                            |
| Not driven | S44, S47                                                                         | `skip: needs P8.8`                            |

The `source` scenarios run first, so S14 can show the `Seen` reports of the `source` room; S4 and S6 run last, because
they leave the room screen and S6 stops the product server. A row moves from `skip` to driven when its phase lands in
the app. The launcher fails when a driven row fails or has no row.

| Scenario | How the launcher proves it                                                                                                                   |
| -------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| S2       | `chat_rooms` names, `chat_room_declarations` description, config and preview, the header                                                     |
| S15–S17  | `/history 74`, reopen: the first `messages_query` is `Before { None }`, newest at the bottom, a tap on a reply quote, paging to `next: None` |
| S18, S11 | `/post text`, `/edit`, `/react`, `/unreact`, `/status`, `/delete` on the screen; `/status` on a message of the user, then Retry              |
| S19–S21  | `/slow 4000`, `/fail reject`, `/fail timeout`: the answers in the event log, the bubble, the notice, the menu                                |
| S46      | `/fail unavailable`, reopen: `NotAvailable`, then the same page asked once more                                                              |
| S22      | `/history 560`, paging past 500 messages, then back to the newest: the dropped pages are asked again                                         |
| S24      | Home, return, Back: `RoomOpened` and `RoomClosed` in the event log, and no worker page in the WebView DevTools list                          |
| S23      | `/fail crash` (a WebView page survives it), then the WebView renderer killed: the cached page stays, Retry, a new worker boot                |
| S1, S3   | `/post legacy` in `chat_messages`; `/meta` in `chat_rooms` and in the open header                                                            |
| S7–S10   | `/post all` and `/help` stored and drawn; `/stream` and `/edit` with one revision row; `/delete` tombstone; reactions table                  |
| S12–S14  | `/echo`, then send, reply, edit, react, delete from the menu; the Actions press; `/commands`, then `/help` as `Command`                      |
| S4, S6   | `/preview` variants in the chat list rows; the list with the product server stopped                                                          |

## Troubleshooting

- The product server listens on `127.0.0.1` of the host, and `adb reverse` connects to it from the adb server. An adb
  server that runs in a sandbox that blocks local connections gives `net::ERR_CONNECTION_REFUSED` in the worker WebView;
  restart it outside the sandbox with `adb kill-server && adb start-server`.
- The app shows "The environment has been reset" after `pm clear`, and the product asks to submit a preimage at its
  first start. The launcher answers "Dismiss" and "Don't allow"; `/post rich` then shows the missing-preimage
  placeholder.
- `TRUAPI_ANDROID_E2E_CHAT_V2_ONLY=S15,S17` runs some scenarios only, and `TRUAPI_ANDROID_E2E_CHAT_V2_REUSE=1` keeps the
  installed app and its data. Both are for work on the launcher; a report from them is not a full run.
