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
| Driven     | S1 to S4, S6 to S14, S49, S53, S54, S55, S61 and S64 to S67 (`host` room), S11, S15 to S24, S46 and S68 (`source` room), S56 and S63 (both rooms) | `pass`, or `fail` with the first failed check |
| Not driven | S62                                                                              | `skip: needs P10.5.1 sample`                  |
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
| S49      | `/caps`: the stored answer has the names that the app logs (`truapi.chat.capabilities`), each with a row above, and the bounds 1 MiB and 0 |
| S61      | the user sends "Crème brûlée at the CAFÉ", `/post all` pushes it out of the screen; the search action of the header finds it for "creme BRULEE" and "café", and a tap shows it in the feed |
| S54      | a swipe to the end and "For 1 hour": `mutedUntil` in `chat_room_declarations`, `UserStateChanged` in the event log (the core keeps it until a message starts the worker), the speaker icon and the grey count in the row; `/notify` and `/notify 3` show nothing (`dumpsys notification`); after the unmute `/notify` shows one. The launcher answers the notifications prompt of the first `/notify` with "Allow always": the core keeps a "Don't allow" for the product, and the mute rule then never runs |
| S55      | a swipe to the start: the "Archived" row and no room row; the room stays archived after `/post text`; the archived section shows it, and a swipe there brings it back |
| S56      | the host room muted always and the source room archived, the app restarted: the first room events of the new worker are both `UserStateChanged` |
| S63      | with no worker page, the list search shows both rooms for "Sample", the host message for "crème", nothing for a text that only the source product keeps, and no worker starts |
| S53      | `/bot add helper Helper`: `bot helper New` and a `chat_bots` row; `/bot post helper …` stored with `originParticipantId` `bot:helper` and drawn under the author "Helper" with no `participants_query`; a repeat answers `Exists` and renames it; `/bot typing helper` gives the activity line "Helper Two: Sample is typing"; `/bot post ghost` is refused with "unknown bot ghost" |
| S64      | `/thread <id>`: the thread room in `chat_rooms` and a `chat_message_meta` row; the chip under the root reads "3 replies" with the unread dot; the list has no thread row; a tap opens "Thread" with the parent name, the pinned root (`chat_thread_root`) and the three replies; Back shows the root. Deleting the parent is not driven: the sample runs commands only in `host`, `source` and its thread rooms |
| S65      | `/branches <id> 3`: a tap on › sends `BranchSelected`, and "Branch 2 of 3" moves the control to 2/3; with `/slow 15000` the next tap shows the spinner, the app logs `truapi.chat.branch … no answer` after 5 s and the control is back at 2/3, then the late answer moves it to 3/3 |
| S66      | `/reason`: the row collapses to "Thought for 4 s" with the content description "Reasoning, collapsed", a tap gives "Reasoning, expanded" and the text, and the list row does not show the reasoning text. The "Thinking…" state of the sample lasts about one second, so a screen dump catches it only sometimes |
| S67      | `/post rich`: no "Read aloud" without `/features readaloud on`; with it, the entry speaks (`truapi.chat.readAloud speak N characters, marks false`), the footer shows Stop, and Back logs `truapi.chat.readAloud stop` |
| S68      | `/branches <id> 3` in the `source` room: a `chat_message_meta` row; after the room closes and loads its pages again through `messages_query`, the control 1/3 is back |
