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
   (`TRUAPI_IOS_E2E_CHAT_COMMANDS`), write the `[e2e:ui]` and runtime markers (`TRUAPI_IOS_E2E_RUNTIME_MARKERS`), skip
   the notification prompt (`TRUAPI_IOS_E2E_NO_PUSH_PROMPT`), grant products the notification permission without a
   prompt (`TRUAPI_IOS_E2E_GRANT_NOTIFICATIONS`, for `/notify`), and stage attachments in a room that declares
   `preimage_attachments` although the app does not list the feature (`TRUAPI_IOS_E2E_STAGING`, for the P10.3 rows).
5. Waits until the product has created its `host` room in Core Data and its bot has started, then opens the `host` room
   through the room link pipeline (`polkadot://<product>/-/chat/host`), so the room screen holds the worker and sends
   `RoomOpened`.
6. Goes through the phases of each scenario. A command goes to the product through the command feed: one JSON file
   `{"room", "text"}` in the app's `tmp/truapi-e2e/commands`, which the app sends to the product as a user message and
   marks `.done` or `.failed`. An action of the user goes to the same feed as `{"room", "host", "argument", "target"}`:
   the simulator-only harness (`TrUAPIE2EHostAction`, `ChatProductRoomPresenter+E2E.swift`) runs the code path of the
   gesture on the room screen that shows the room (`open` through the room link pipeline, `list`, `send`, `menu`,
   `confirm`, `toolbar`, `participants`, `participant`, `reactions`, `suggestion`, `type`, `attach`, `button`,
   `footer-action`, `older`, `jump`, `retry`, `retry-room`, `back`, `bottom`, and for P10 `room-mute <1h|8h|1w|always>`,
   `room-unmute`, `room-archive`, `room-unarchive` (the entries of the header menu), `search-open`, `chip`, `branch`,
   `reasoning`, `delete <message>`, the message menu actions `reply <text> <message>`, `edit <text> <message>` and
   `react <emoji> <message>` (a second `react` with the same emoji removes it), `dump` (the marker `dump` gives the
   window as one JSON array: id, author, the name of the author, time, status, text, `replyTo` and the reaction
   emoji), the call actions `call-button <audio|video>`, `call-answer <accept|decline>`,
   `call-open`, `call-end` and `call-timer`, and the staging actions `notice <accept|cancel>`, `notice-reset` and
   `outbox-clear`). The search
   screen of a room takes `search <text>`, `search-more` and `search-select <first|last|product id>`. The chat list
   takes `list-mute`, `list-unmute`, `list-archive`, `list-unarchive` (its swipe actions and menu entries), `archived`
   (opens the archived rooms) and `list-search <text>` (its search field). Three actions act on the app instead of one
   room: `release-worker` and `hold-worker` give back and take again the worker hold of the harness, and
   `list-render` draws the chat list again from what the app stores. `wake <watch id>` stands in for the receiving
   layer (O28) and wakes the product as an authenticated notification will. `permission <always|once|deny>` answers the
   permission prompt on the screen, `permission-reset` clears a kept `PreimageSubmit` refusal, and `staging-fault`
   makes the next stage fail as a failed submit (S59). Three actions measure: `frames <seconds>` runs the
   frame meter on the room screen, `participants-scroll <rows>` scrolls the participant list to its end, and
   `b7 <runs>` times 8 MiB across the binding into the core. The
   launcher then waits for each expectation: Core Data rows, the event log of the product, or the
   `[e2e:ui] <key> <room> <value>` console markers that the app writes when what it shows changes (`window`,
   `readiness`, `toolbar`, `menu`, `footer`, `header`, `author-<id>`, `sheet`, `reactions`, `suggestions`, `catalogue`,
   `activity`, `placeholder`, `allowed`, `failure`, `policy`, `screen`, `chat-list`, `perf-ready`, `frames`,
   `perf-participants`, and for P10 `chat-archive`, `archived-row`, `row`, `search-action`, `search`, `jumped`,
   `list-search`). An expectation `consoleNot` checks that the console has no line with a text, for example that no
   notification was scheduled or that no worker started. It takes a screenshot at the end of each scenario and where a phase asks for one.
7. Writes `playground/test-results/ios-chat-v2/diagnosis-report.md`, `event-log.jsonl`, `app.log` and the screenshots.

`TRUAPI_IOS_E2E_ONLY=S3,S25` drives the named scenarios only. The app keeps its data between runs, so a scenario checks
what changed in the run and not absolute counts.

The launcher builds the sample product with `yarn build`. When `yarn` is not on the `PATH`, put a shim that runs
`npx -y yarn@1.22.22 "$@"` before it, or build the product first and set `TRUAPI_IOS_E2E_SKIP_PRODUCT_BUILD=1`.

## Rows

The report has one row for each scenario of the test plan that names the iOS layer (04-TESTING section 4). The steps and
the checks of each row are in `scripts/lib/ios-chat-v2-scenarios.mjs`. Every row is driven:

| Scenarios                            | How the launcher proves the row                                                                                                                                                                                                              |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S1 to S4, S7 to S22, S24 to S41, S46 | Core Data rows, the event log of the product and the `[e2e:ui]` markers, as above                                                                                                                                                            |
| S5                                   | `/room create` makes the room `throwaway`; with its screen open, `/room delete` removes the row and the screen closes (`screen removed`)                                                                                                     |
| S6                                   | The list screen and `release-worker` let the core stop the worker (`[truapi] worker stopped`); `list-render` draws the product rooms from Core Data                                                                                          |
| S23                                  | `/fail crash` makes the product throw outside a promise; the worker ends, the room shows `unavailable` with its window, and the next request (`retry-room`, or a `Seen` report that comes first) starts a new worker that hears `RoomOpened` |
| S44                                  | `/flood` sends 25 edits at once; the core refuses the edits past 20 (`refused>=1`), the last edit is stored, and the `frames` marker gives the dropped frames and the number of list updates                                                 |
| S49, S51, S53 to S69, S72 to S74     | The rows of the Chat Extension v2 additions (P10); they run last, as below. S57 is a reasoned skip, and two rows S57u-host and S57u-source drive it under the live bound |
| S47                                  | S47 changes `allowed_reactions` twice on an open room; each change writes a new `policy` generation marker without a reopen                                                                                                                  |

The launcher fails when a driven row fails or has no row.

The rows of the Chat Extension v2 additions (P10) run last, S49 at the end. S51 declares two features that the app
does not serve and reads the stored bytes in `CDChatRoomMetadata.ZFEATURES`; the run does not restart the app. S54
mutes the `host` room from its list row for 1 hour: the product receives `UserStateChanged` (with `/echo` on), the row
draws the muted state, and a `/notify` into the room schedules no notification; a `/notify` before the mute does. S55
archives the room from the list, checks that it leaves the list for the archived list and stays there after
`/post text`, unarchives it, and deletes an archived `throwaway` room. S56 mutes `host` and archives `source` from the
header menu, lets the worker stop and holds it again: the new worker receives both states. S61 searches the `host`
room for its user's "Crème brûlée" message with "CREME BRULEE" and jumps to it. S62 checks that `source` has no search
action until `/search on`, pages two `search_query` answers of 30 hits, jumps to an older hit, and shows "not available"
after `/search na`. S63 searches the chat list with the worker stopped: room names, the text of the `host` room, never
the messages of `source`, and no `worker running` line. These rows need the sample product with `/notify` and
`/search` (P10.10.1, P10.5.1). The room-screen rows use the harness actions `chip`, `branch` and `reasoning` and these markers:

- S53: `/bot add` and `/bot post` give the `personas` marker and an `author-bot:<id>` marker with the name of the
  persona and `icon` when its icon loaded; the host room declares no participants, so no `participants_query` names the
  author. `/bot typing` gives an `activity` marker with the name of the persona.
- S64: `/thread` gives a `chip-<id>` marker; the `chat-list` marker after `list-render` does not name the thread room;
  `chip` opens the thread room (`thread` and `root` markers); `back` gives `returned` in the parent room. The sample
  takes commands only in its host, source and thread rooms, so the deletion cascade of `delete_room` is proven by the
  unit test `ChatRoomAdditionsServiceTests/deletingARoomDeletesItsThreadRooms` and not on the simulator.
- S65: `branch-<id>` markers: the position, `pending=<index>` while the arrow waits, and `reverted` after 5 s with
  `/slow 6000`, then the late answer.
- S66: `reasoning-<id>` (`thinking`, `collapsed Thought for 4 s`, `expanded`) and `a11y-reasoning-<id>` (the VoiceOver
  label); `previewNot` reads `ZLASTDISPLAYMESSAGE` of the chat.
- S67: the `menu` marker lists `readAloud` only after `/features readaloud on`; `readaloud` gives the plain text that
  the app speaks, `readaloud-audio started` comes from the speech synthesizer, and `readaloud stopped` comes when the
  user leaves the room.
- S68: `/history 700` and two `jump` actions move the newest message out of the window of 500 and back; the
  `meta-drawn` marker names the messages of the window that have meta.
- S69: with the hold of the harness given back, a message sent and the list opened at once still gets the echo of the
  product (wake W1); the worker stops 30 s after the wake and no sooner; a message to a stopped worker wakes it.
- S72: `/features calls both` shows both header buttons; `call-button video` sends `CallRequested`, answered with an
  `Active` call; `/features calls off` removes the buttons.
- S73: `/call ring` shows the ringing banner in the room; on the chat list `/call ring video` gives the "Incoming call"
  row and the in-app banner; `call-answer accept` and `decline` send `CallAnswered`; a call that nobody answers clears
  after 60 s.
- S74: `/call active` shows the call bar; `call-timer` reads its timer, `call-open` sends `CallOpened` and `call-end`
  sends `CallEnded`. The background notification of a ringing call has unit tests only.
- S57 to S60 (P10.3) stage on the live Paseo Next Bulletin chain. The harness identity is a new wallet with no
  personhood, so the core gets no Bulletin allowance for the product (O34) and every upload stops at that step; the
  rows check each step before it. S60 runs first: the public-storage notice shows for the first staged attachment of
  the product and not for the second, and the first `PreimageSubmit` prompt is answered with `permission always`.
  S57u-host (a 1.8 MiB file) and S57u-source (a 1.5 MiB photo) reach the core under the 2 MiB bound of the live chain
  (O32) and stay Failed at the allowance. S59 fails a stage with `staging-fault`, retries it and deletes it. S58
  checks the refusals: no `preimage_attachments`, a file above 8 MiB, a file above the live bound (after which the room
  refuses a file above it before it goes to the core), and `PreimageSubmit` refused (`permission deny`, then
  `permission-reset`). S57 itself (3 MiB and 6 MiB) is a skip until O32 and O34 are settled.

S49 posts
`/caps` and expects exactly the names of `ChatHostCapabilities.served` (`IOS_CHAT_FEATURE_ROWS` in the scenario
library), then checks that every row of each listed feature passed in the same run. With `TRUAPI_IOS_E2E_ONLY` the row
names the feature rows that the run did not drive.

## Performance rows

`TRUAPI_IOS_E2E_PERF=1` also drives the rows B1 to B3, B7 and B9 of 04-TESTING section 5 after the scenarios. The numbers
are in the evidence of each row. The app measures them with a simulator-only frame meter (`TrUAPIE2EFrameMeter`, a
`CADisplayLink`): a frame time of 1.5 frames or more is a dropped frame.

| Row | What it does                                                                          | Marker                                                     |
| --- | ------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| B1  | `/participants 10000`, opens the list, scrolls it to row 10,004 at 300 points a frame | `perf-participants`: frames, p95, max, dropped, footprint  |
| B2  | `/history 50000`, opens the room (warm), then ends the worker and retries (cold)      | `perf-ready`: milliseconds from the open to a ready window |
| B3  | the open room idle, then `/stream`, then `/flood`, each with the frame meter on       | `frames`: frames, p95, max, dropped, list updates          |
| B7  | `b7 7`: 8 MiB across the binding into the core, seven times; the upload is not timed (no Bulletin allowance, O34) | `b7`: bytes and median                                     |
| B9  | the worker stopped, then `wake` from the chat list, twice cold and once warm           | the stored preview "Woken by <watch id>", timed            |

B4, B6 and B8 are unit tests that measure: run `ChatResourcePerformanceTests` (B4, B6) or `ChatSearchPerformanceTests`
(B8) with `TEST_RUNNER_TRUAPI_CHAT_PERF=1` and, for a file with the numbers, `TEST_RUNNER_TRUAPI_CHAT_PERF_OUT=<path>`. The numbers of the last run are in
`HOST-CHAT-EXTENSION-V2/results/RESULTS.md`.
