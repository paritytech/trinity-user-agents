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
2. Installs the APK (the package comes from `aapt2`, or `TRUAPI_ANDROID_E2E_PACKAGE`), clears its data, grants the
   notification and camera permissions, puts the S36 attachments (`sample.png` of the sample product and a 2 MiB
   `large.bin`) into the Download folder of the device, turns the on-screen keyboards off (and closes one that the
   system turns on again before each screenshot), and records the whole logcat into `app.log`.
3. Starts the app and sends the debug-only `E2EHookReceiver` broadcast: it seeds a wallet and a lite username (no
   onboarding taps) and installs `truapi-chat-v2.dot` as a chat product with the served worker. Then it restarts the app
   and waits until the product has created its `host` and `source` rooms in `chat_rooms`, and until the worker logs its
   preimage line (the end of its start; the launcher answers the preimage prompt with "Don't allow"). When either does
   not come in time, the run stops: the launcher does not restart the app.
4. Runs each scenario as a user: it opens a room with the chat deeplink `polkadotapp://chat?chatId=<hex>`, types the
   commands of the sample product into the room input (the `clipboard` hook puts the text on the clipboard and the
   launcher pastes it), taps buttons, menu entries, chips, sheets and the system document picker that it finds with
   `uiautomator dump`, and scrolls with swipes. The S4 commands of the `host` room go through the `message` hook while
   the chat list is on screen, and so do the commands that restore the input after S34 and S35 hide it.
5. Proves each row from the event log of the worker (its `console.info` lines, which the app logs under the tag
   `ProductWebChromeClientKt`), the app database (`run-as <package> sqlite3 -json databases/app_v2.db`), the screen and
   a screenshot.
6. Writes `playground/test-results/android-chat-v2/diagnosis-report.md` (gitignored) with `event-log.jsonl`, `app.log`
   and the screenshots `sNN-<step>.png`.

A room that opens soon after another one closed restarts the worker. The app waits for the client of the new worker to
connect before it asks for a page, so the launcher does not wait for the worker and taps Retry only where a scenario
asks for it (S11, S21, S23).

## Rows

The report has one row for each scenario of the test plan that names the Android layer (04-TESTING section 4), in the
row format of the iOS report.

| Kind       | Scenarios                                                                                                                                                                                  | Row                                           |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------- |
| Driven     | S1 to S4, S6 to S14, S25 to S29, S33 to S41, S47, S49, S53 to S55, S58, S61, S64 to S67, S69 and S72 to S74 (`host` room); S11, S15 to S24, S30 to S32, S46, S59 and S68 (`source` room); S56, S57, S60 and S63 (both rooms) | `pass`, or `fail` with the first failed check; S57 and S59 give `skip` with the reason when the emulator chain cannot carry them |
| Benchmark  | B9, only with `TRUAPI_ANDROID_E2E_PERF=1` or `B9` in `TRUAPI_ANDROID_E2E_CHAT_V2_ONLY`, after the scenarios | `pass` with the times, or `fail` |
| Not driven | S5 (the sample has no command that calls `delete_room`)                                                                                                                                    | `skip` with that reason                       |
| Not driven | S44 (no command of the sample sends more than 20 updates in a second)                                                                                                                      | `skip` with that reason                       |
| Not driven | S62                                                                                                                                                                                        | `skip: needs P10.5.1 sample`                  |

The `source` scenarios run first, so S14 can show the `Seen` reports of the `source` room. The P10 rows come after the
P8 rows, and S49 comes after every row that proves a feature that the app can list. S4 and S6 run last, because they
leave the room screen and S6 stops the product server. Each P8.6 scenario puts the room back as it found it
(`/menu none`, `/toolbar 0`, `/footer input`, `/placeholder`, `/suggest off`, `/commands off`, `/activity off`,
`/allowed`, `/reactions off`). The launcher fails when a driven row fails or has no row.

| Scenario | How the launcher proves it                                                                                                                                                                          |
| -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S2       | `chat_rooms` names, `chat_room_declarations` description, config and preview, the header                                                                                                            |
| S15–S17  | `/history 74`, reopen: the first `messages_query` is `Before { None }`, newest at the bottom, a tap on a reply quote, paging to `next: None`                                                        |
| S18, S11 | `/post text`, `/edit`, `/react`, `/unreact`, `/status`, `/delete` on the screen; `/status` on a message of the user, then Retry                                                                     |
| S19–S21  | `/slow 4000`, `/fail reject`, `/fail timeout`: the answers in the event log, the bubble, the notice, the menu                                                                                       |
| S46      | `/fail unavailable`, then a scroll to older messages in the open room: `NotAvailable`, then the same page asked once more (a reopen would ask the participant count first)                          |
| S22      | `/history 560`, paging past 500 messages, then back to the newest: the dropped pages are asked again                                                                                                |
| S24      | Home, return, Back: `RoomOpened` and `RoomClosed` in the event log, and no worker page in the WebView DevTools list                                                                                 |
| S23      | `/fail crash` (a WebView page survives it), then the WebView renderer killed: the cached page stays, Retry, a new worker boot                                                                       |
| S1, S3   | `/post legacy` in `chat_messages`; `/meta` in `chat_rooms` and in the open header                                                                                                                   |
| S7–S10   | `/post all` and `/help` stored and drawn; `/stream` and `/edit` with one revision row; `/delete` tombstone; reactions table                                                                         |
| S12–S14  | `/echo`, then send, reply, edit, react, delete from the menu; the Actions press; `/commands`, then `/help` as `Command`                                                                             |
| S30, S31 | `/participants 6`: the header count without a reopen, authors from `ByIds`, the list pages; the sheet of `p4` and its Mention action                                                                |
| S32      | `/reactions on`, `/react`: a long press on the chip lists who reacted; `/reactions na`: `NotAvailable`, no list, and no second query                                                                |
| S25–S28  | `/menu extend`, `hide-edit`, `custom` in the long-press menu; `/slow 6000` falls back to the defaults; Pin sends `MessageActionTriggered`; `/deeplink` asks "Open this link?" and hands the link on |
| S29      | `/toolbar 4`: two buttons inline, two in More actions; each press sends `ToolbarActionTriggered`                                                                                                    |
| S33–S36  | `/footer input`, `photo`, `empty`, `custom` on the screen; `sample.png` through Attach and the document picker goes inline, `large.bin` is refused                                                  |
| S37–S41  | `/placeholder` in `host-empty`; `/suggest` chips; the `/` catalogue; `/activity` and `RoomClosed`; `/allowed 👍❤️` in the picker                                                                    |
| S47      | `configGeneration` grows on `/menu extend` and `/menu none` while the room stays open, and the menu follows each one                                                                                |
| S61      | the user sends "Crème brûlée at the CAFÉ", `/post all` pushes it out of the screen; the search action of the header finds it for "creme BRULEE" and "café", and a tap shows it in the feed |
| S54      | a swipe to the end and "For 1 hour": `mutedUntil` in `chat_room_declarations`, `UserStateChanged` in the event log (the core keeps it until a message starts the worker), the speaker icon and the grey count in the row; `/notify` and `/notify 3` show nothing (`dumpsys notification`); after the unmute `/notify` shows one. The launcher answers the notifications prompt of the first `/notify` with "Allow always": the core keeps a "Don't allow" for the product, and the mute rule then never runs |
| S55      | a swipe to the start: the "Archived" row and no room row; the room stays archived after `/post text`; the archived section shows it, and a swipe there brings it back |
| S56      | the host room muted always and the source room archived, the app restarted: the first room events of the new worker are both `UserStateChanged` |
| S60      | `/footer photo` and `/features preimage on` in both rooms; a 1.5 MB photo picked through the system picker (the file goes to `Download` with `adb push`) shows the public-storage notice; Cancel stages nothing and keeps no outbox entry; the notice shows again, Send stages; a file in the `source` room shows no notice |
| S57      | a 3 MiB photo and a 6 MiB file in the `host` room: the stage line of the app (`truapi.chat.stage`) and the banner; then a photo and a file under 2 MiB in both rooms: `media` and `preimage` `Fetched` in the event log. B7: the `stage_probe` hook times 8 MiB across the binding. `skip` when the chain bound refuses the S57 sizes (O32) or the core has no Bulletin allowance for the product |
| S59      | PreimageSubmit refused: the bubble is Failed with the banner, the outbox keeps the bytes; after a reset of the decision, Retry in the message menu stages again; Delete removes the bubble and the folder of the entry in `files/product_attachment_outbox` |
| S58      | `/features preimage off`: "Choose one under 1 MB"; a file of 8 MiB + 1 B: "Choose one under 8 MB"; PreimageSubmit refused: `PermissionDenied`, no `media` line in the event log |
| S63      | with no worker page, the list search shows both rooms for "Sample", the host message for "crème", nothing for a text that only the source product keeps, and no worker starts |
| S53      | `/bot add helper Helper`: `bot helper New` and a `chat_bots` row; `/bot post helper …` stored with `originParticipantId` `bot:helper` and drawn under the author "Helper" with no `participants_query`; a repeat answers `Exists` and renames it; `/bot typing helper` gives the activity line "Helper Two: Sample is typing"; `/bot post ghost` is refused with "unknown bot ghost" |
| S64      | `/thread <id>`: the thread room in `chat_rooms` and a `chat_message_meta` row; the chip under the root reads "3 replies" with the unread dot; the list has no thread row; a tap opens "Thread" with the parent name, the pinned root (`chat_thread_root`) and the three replies; Back shows the root. Deleting the parent is not driven: the sample runs commands only in `host`, `source` and its thread rooms |
| S65      | `/branches <id> 3`: a tap on › sends `BranchSelected`, and "Branch 2 of 3" moves the control to 2/3; with `/slow 15000` the next tap shows the spinner, the app logs `truapi.chat.branch … no answer` after 5 s and the control is back at 2/3, then the late answer moves it to 3/3 |
| S66      | `/reason`: the row collapses to "Thought for 4 s" with the content description "Reasoning, collapsed", a tap gives "Reasoning, expanded" and the text, and the list row does not show the reasoning text. The "Thinking…" state of the sample lasts about one second, so a screen dump catches it only sometimes |
| S67      | `/post rich`: no "Read aloud" without `/features readaloud on`; with it, the entry speaks (`truapi.chat.readAloud speak N characters, marks false`), the footer shows Stop, and Back logs `truapi.chat.readAloud stop` |
| S68      | `/branches <id> 3` in the `source` room: a `chat_message_meta` row; after the room closes and loads its pages again through `messages_query`, the control 1/3 is back |
| S69      | `/echo`, a message sent and Back at once: the wake of the message in logcat (`TrUAPI <product> woken for message-in-flight host`), the echo stored; the worker page goes 30 s or more after the room closed; a `message` hook to the stopped worker wakes it and gets its echo |
| S72      | `/features calls both`: both header buttons (`chat_call_button_audio`, `chat_call_button_video`); a press sends `CallRequested` and the product answers with a call bar; `audio` shows one button, `off` none |
| S73      | `/call ring`: the banner of the room; `/call ring video` from the list: the row shows "Incoming video call" and the in-app banner shows; Home: the `CallStyle` notification in `dumpsys notification` (and Answer in the shade, when the shade shows it); Accept and Decline in the room send `CallAnswered`; a call that nobody answers goes after 60 s |
| S74      | `/call active`: the bar with its timer; a tap on the bar sends `CallOpened`, End sends `CallEnded` |
| S49      | `/caps`: the stored answer has the names that the app logs (`truapi.chat.capabilities`) and the bounds 1 MiB and 0 (8 MiB when the app lists `preimage_attachments`); each row that proves a listed name (`ANDROID_CHAT_FEATURE_ROWS`) passed earlier in the run |
| S4, S6   | `/preview` variants in the chat list rows; the list with the product server stopped                                                                                                                 |
| B9       | on the chat list with no worker page, the `wake` hook (`ChatWakeReason.Notification`, in place of the receiving layer, O28) to the stored preview "Woken by <watch>": two cold wakes and one warm wake |

## Troubleshooting

- The product server listens on `127.0.0.1` of the host, and `adb reverse` connects to it from the adb server. An adb
  server that runs in a sandbox that blocks local connections gives `net::ERR_CONNECTION_REFUSED` in the worker WebView;
  restart it outside the sandbox with `adb kill-server && adb start-server`.
- A screen dump of a busy emulator can take more than 20 s. The launcher waits up to 90 s for one, and stops a dump that
  does not end. When an adb command times out, the helpers restart the adb server; the logcat stream then connects again
  from its last line, and the launcher maps the product port again.
- An emulator under load can drop a swipe gesture. When a mute or archive swipe on a list row changes nothing in 6 s,
  the launcher swipes again, at most three times, and notes each repeat under "Observed while driving the app".
- The app shows "The environment has been reset" after `pm clear`, and the product asks to submit a preimage at its
  first start. The launcher answers "Dismiss" and "Don't allow"; `/post rich` then shows the missing-preimage
  placeholder. A permission prompt that no screen can show, or that a navigation removes before an answer, ends the
  request with an error, not with a refusal: the start of the product never waits on it, the core keeps no decision,
  and the product can ask again.
- S27: the deeplink handlers of the app open only names of the network that the app uses (`.testnet` on the test
  networks), and the sample product is a `.dot` name, so the app hands the link to the system, which has no handler. The
  row proves the confirmation and the hand-over, not an App page.
- Two runs on one host need their own ports: `TRUAPI_ANDROID_E2E_CHAT_PRODUCT_URL=http://127.0.0.1:<port>` for the
  product server and `TRUAPI_ANDROID_E2E_DEVTOOLS_PORT` for the WebView DevTools socket.
- The P10.3 rows use two debug-only hooks of `E2EHookReceiver`: `serve_preimage_attachments` makes the app stage the
  attachments of a room that declares `preimage_attachments` also while the app does not list the feature, and
  `reset_preimage_permission` clears the stored PreimageSubmit decision of the product (the launcher answers "Don't
  allow" at the first start). The launcher answers each later PreimageSubmit prompt as the row needs it. The core does
  all staging; the hooks hold no key and open no connection.
- `TRUAPI_ANDROID_E2E_CHAT_V2_ONLY=S15,S17` runs some scenarios only, and `TRUAPI_ANDROID_E2E_CHAT_V2_REUSE=1` keeps the
  installed app and its data. Both are for work on the launcher; a report from them is not a full run.
