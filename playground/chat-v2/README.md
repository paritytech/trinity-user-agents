# Chat Extension v2 sample product

_The one product every host and tool runs to test [Chat Extension v2](../../docs/rfcs/chat-extension-v2.md)._

This file is the specification of the code in this directory.

## Purpose

The sample product exercises every method of the `Chat` trait, ids `0` to `4`, `6` and `7` to `21`, in both room
modes. Each feature is a slash command, so a tester or a launcher triggers it by typing in a room. The same command
list runs on the test host, the CLI chat page, the iOS simulator and the Android emulator, and every run produces the
same event log, so the results compare across hosts row by row.

The product is a test fixture. It is not an example of how to structure a chat product.

## Layout

The product follows the layout of the [TrUAPI Playground](../README.md): a static App and a Worker nested at
`out/worker`, built with `yarn build`, which is what the iOS and Android chat launchers run and look for.

```text
playground/chat-v2/
  package.json               yarn scripts: dev, build, test, lint; @parity/truapi linked from ../../js/packages/truapi
  bulletin-deploy.config.ts  the product manifest: one app and one worker executable
  vite.config.app.ts         builds app/ into out/; `yarn dev` serves it on port 3102
  vite.config.worker.ts      builds worker/index.ts into out/worker/index.js
  vite.config.shared.ts      the DotNS name, from CHAT_V2_DOTNS_NAME, for the room links and the deeplink
  app/index.html             the one-page App
  app/main.ts
  worker/index.ts            the entry: the client of the Worker and the assets, given to startSampleProduct
  worker/product.ts          start-up: handlers, subscriptions, rooms; events, user actions, renderer
  worker/execute.ts          the command implementations
  worker/commands.ts         the command parser and the command table
  worker/feed.ts             feed updates, mirrored into the store in the source room
  worker/clock.ts            message ids and timestamps from counters
  worker/store.ts            the product-source room's messages and reactions, and its pages
  worker/queries.ts          answers to the six host-initiated queries
  worker/fixtures.ts         participants, menus, toolbar, bodies, room declarations, generated history
  worker/state.ts            the state kept in product local storage
  worker/render.ts           the Custom message and Custom footer trees
  worker/log.ts              the event log
  worker/assets.ts           the assets, inlined by Vite
  assets/                    avatar.png, avatar-alt.png, badge.png, icon-1.png to icon-6.png, sample.png, sample.pdf, sample.mp4
  tests/unit/                bun tests of the pure modules, and of the Worker against a fake client
  tests/e2e/                 bun tests of the Worker on the test host: the real core (WASM) over the mock host of
                             @parity/truapi-host/testing, with the mock host as the user; one test per scenario
  tests/support/             the fake client, and the start of the Worker on the test host
```

`bulletin-deploy.config.ts` declares the product as the playground's does:

```ts
export default {
  domain: process.env.CHAT_V2_DOTNS_NAME ?? "truapi-chat-v2.paseo",
  displayName: "Chat Extension v2 Sample",
  description: "Exercises every Chat Extension v2 method in a host-store and a product-source room.",
  icon: { path: "assets/avatar.png", format: "png" },
  executables: [
    { kind: "app", path: "out", appVersion },
    {
      kind: "worker",
      path: "out/worker",
      appVersion,
      entrypoint: "index.js",
      includes: { chat: true, pocket: false },
    },
  ],
};
```

Every asset is smaller than the inline bounds of the RFC: images and icons under 64 KiB, `sample.pdf` and `sample.mp4`
under 1 MiB. The Worker imports them as bytes, so they travel as `ChatResource::Inline`.

### The App

One page, with no framework:

- Two buttons, "Open host room" and "Open source room", which call `system.navigate_to` with the room links
  `polkadot://<product>/-/chat/host` and `polkadot://<product>/-/chat/source`.
- The command table of this file, rendered from the same `worker/commands.ts` the Worker uses.
- A banner "Opened from a deeplink" when the page URL carries `?from=deeplink`, which the `/deeplink` and `/toolbar`
  commands use as their target.

The App calls no `Chat` method: `Chat` is a Worker service.

### The Worker

At start the Worker does, in order, before its first `await` (the state load starts first, so that a query the client
kept for a handler, which the client gives during the registration, waits for the state):

1. Registers a handler for each of the six queries (`messages_query`, `user_action`, `reactions_query`,
   `participants_query`, `message_actions_query`, `participant_actions_query`) and `Renderer::render`.
2. Opens `Chat::action_subscribe` and `Renderer::action_subscribe`.
3. Calls `create_room` (V2) for the two rooms of the next section. A room that answers `Exists` is not declared again.

It keeps its state, the counters, the `source` room's messages and each command's current setting, in product local
storage, so a worker restart continues where the last run stopped.

## Rooms

| Room     | Mode           | Declared at creation                                                                                                    |
| -------- | -------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `host`   | Host-store     | Metadata only: name "Sample: host store", avatar `avatar.png`, description. Default configuration, no preview.           |
| `source` | Product-source | Name "Sample: product source", avatar `avatar.png`; `product_source`, `reaction_details`, `participants`, `message_actions`, `participant_actions` and `report_seen` all `true`; preview `Timestamp` with badge `UnreadCount { count: 0 }` |

The `source` room starts with no messages. A command applies to the room it is typed in, and the product answers in that
room. A command typed in a room where it has no meaning answers `error` and makes no call.

How a command reaches the product depends on the room and on whether the room declared its commands:

| Room     | Commands not declared                                          | Commands declared with `/commands`      |
| -------- | -------------------------------------------------------------- | --------------------------------------- |
| `host`   | `ChatEvent::MessagePosted` with the text                       | `ChatEvent::Command`                    |
| `source` | `user_action` `NewMessage` with the text, answered `MessageCreated` | `ChatEvent::Command`               |

The product parses a text whose first word starts with `/` as a command, and treats a `Command` event the same way.

## Determinism

Every value the product sends is a function of the commands it received, so two hosts that receive the same commands
see the same messages.

- Room ids are `host`, `source` and `host-empty`.
- The product's own messages have the id `<room>-p-<n>`, with `<n>` a four-digit counter per room starting at `0001`.
  The messages the user sends in `source` have the id `source-u-<n>`. Generated history has the id `source-h-<n>`, with
  a five-digit counter.
- Timestamps come from one counter: the `n`-th message the product creates has the timestamp
  `1_767_225_600_000 + n * 1_000` (2026-01-01T00:00:00Z plus `n` seconds). History is older than every live message:
  message `i` of `n` has `1_767_225_600_000 - (n - i) * 60_000`.
- Texts are fixed strings with the counter in them, such as "History message 00017". `/stream` sends the words of one
  fixed sentence.
- Fixed participants: `p1` is `Person { person_id: 1 }`, `p2` is `LitePerson` with the account `0x02` repeated 32
  times, `p3` is `Contact` with the handle `0x03` repeated 32 times, `p4` is `Product { product_id:
  "truapi-playground.dot" }`. `/participants <n>` adds `p5` onward as `Custom { nickname: "Guest <n>", avatar: None }`.
- Action ids: menu entries `act-pin`, `act-report`, `act-mention`, `act-open-app`; buttons `act-yes`, `act-no`;
  toolbar buttons `tb-1` onward; quick replies `sg-send`, `sg-event`; the custom footer button `footer-ping`; the custom
  message button `card-like`.
- `<id>` in a command is a message id or `last`, the newest message of the product in the room.
- The product never reads the clock or a random source for anything it sends. Ids the host assigns, the user's messages
  in `host`, are the only values that differ between hosts, and the diagnosis compares them by kind, not by value.

The one exception is the preimage image of `/post rich`. At first start the Worker submits `assets/sample.png` through
`Preimage::submit` and keeps the key. A host that refuses the submit leaves the key `0x00` repeated 32 times, which no
preimage has, so the message tests the missing-preimage placeholder instead. The event log records which key was used.

## Commands

A command that posts no message of its own answers with one `Text` message `ok <command>` from `"product"` once its
calls succeed, or `error <command>: <reason>` when one fails. In `host` the product posts with `update_feed` `New`; in
`source` it stores the message and pushes the same `update_feed` `New`. "Both" in the Room column means the command
runs in either room.

| Command                                            | Room   | Wire: method and payload                                                                                                                                                                                                              | Expected host behaviour                                                                                                     | Scenarios     |
| -------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | ------------- |
| `/help`                                            | Both   | `update_feed` `New` with `RichText`: a Markdown list of every command, with bold names and inline code                                                                                                                                 | Draws the list with Markdown                                                                                                 | S7            |
| `/meta <name>`                                     | Both   | `set_room_metadata { name: <name>, avatar: Inline avatar-alt.png, description: "Renamed by /meta" }`                                                                                                                                   | The list row and the open room's header change without reopening                                                           | S3            |
| `/preview pin`, `unpin`                            | Both   | `set_room_preview` with `order: PinToTop` or `Timestamp`; every other field as last set                                                                                                                                              | The room moves above or back among the unpinned rooms                                                                        | S4            |
| `/preview badge <n>`, `image`, `none`              | Both   | `set_room_preview` with `badge: Some(UnreadCount { count: n })`, `Some(Image { badge.png })` or `Some(Hidden)`                                                                                                                         | The badge shows the number, the image, or nothing; `badge 0` shows nothing                                                  | S4            |
| `/preview typing on`, `off`                        | Both   | `set_room_preview` with `text: Some({ text: "Sample is typing", ongoing: true })`, or `text: None`                                                                                                                                   | The list row shows the animated line, then the newest message again                                                         | S4            |
| `/preview text <t>`                                | Both   | `set_room_preview` with `text: Some({ text: <t>, ongoing: false })`                                                                                                                                                                  | The list row shows `<t>`                                                                                                     | S4            |
| `/post text`                                       | Both   | `update_feed` `New` with `Text`                                                                                                                                                                                                      | A plain bubble from the product, links tappable                                                                             | S7            |
| `/post rich`                                       | Both   | `update_feed` `New` with `RichText`: Markdown of every supported element plus a heading and a table, media `[Image Inline sample.png, Image Preimage <key>]`, each with width, height, BlurHash and `alt`                                 | Markdown drawn per the RFC subset, the table as text; the inline image at once, the preimage image after its BlurHash        | S7            |
| `/post video`                                      | Both   | `update_feed` `New` with `RichText`, media `[Video Inline sample.mp4]` with a BlurHash poster                                                                                                                                        | A poster that plays in the host player                                                                                       | S7            |
| `/post file`                                       | Both   | `update_feed` `New` with `RichText`, media `[File Inline sample.pdf, file_name: "sample.pdf"]`                                                                                                                                         | A file row with name, size and icon; a tap opens the share sheet                                                             | S7            |
| `/post actions`                                    | Both   | `update_feed` `New` with `Actions { text: "Pick one", actions: [act-yes, act-no], layout: Column }`; a press is answered with `Edited` to `Actions { text: "Picked <choice>", actions: [] }`                                              | Two buttons; a press sends `ActionTriggered`, then the buttons disappear                                                    | S7, S13       |
| `/post custom`                                     | Both   | `update_feed` `New` with `Custom { message_type: "chat-v2-card", payload: <counter>, preview_text: "Sample card" }`; `Renderer::render` answers a title, the counter and a `card-like` button                                       | The product's tree in the bubble; "Sample card" in the chat list; a host without `Renderer` draws "Sample card"            | S7, S48       |
| `/post legacy`                                     | `host` | `post_message` V2 with `Text`                                                                                                                                                                                                        | A product bubble with a host id; in `source` the host refuses the call and the product answers `error`                      | S1            |
| `/post all`                                        | Both   | Each `/post` kind above, in order                                                                                                                                                                                                    | Every body kind in one screen                                                                                                | S7            |
| `/stream`                                          | Both   | `New` with `Text` and `status: Preparing`, forty `Edited` at 20 per second each adding one word, then `StatusChanged { Sent }`                                                                                                        | One bubble grows in place without a dropped frame, then shows its sent mark                                                 | S8            |
| `/edit <id> <text>`                                | Both   | `update_feed` `Edited { body: Text <text>, edited_at: <tick> }`                                                                                                                                                                       | The bubble changes in place and shows "edited"                                                                              | S8            |
| `/delete <id>`                                     | Both   | `update_feed` `Deleted`                                                                                                                                                                                                              | The bubble goes; the list preview recomputes                                                                                 | S9            |
| `/react <id> <emoji>`, `/unreact <id> <emoji>`     | Both   | `update_feed` `ReactionPlaced` or `ReactionRemoved` with `participant_id: "p1"`                                                                                                                                                       | A chip appears or its count drops; the user's own reaction on the same emoji aggregates with it                             | S10           |
| `/status <id> sent`, `delivered`, `read`, `failed` | Both   | `update_feed` `StatusChanged`                                                                                                                                                                                                        | The status mark changes; `failed` offers retry in `source` and delete in `host`                                              | S11           |
| `/menu custom`                                     | Both   | `set_room_config` with `message_actions: true`; `message_actions_query` answers `Custom { [act-pin, act-report (destructive)] }`                                                                                                       | A long press shows only Pin and a red Report, last                                                                          | S26, S28      |
| `/menu extend`                                     | Both   | as `custom`, answering `ExtendBuiltIn { actions: [act-pin], hidden: [] }`                                                                                                                                                              | The built-in entries, then Pin in its own section                                                                           | S25, S28      |
| `/menu hide-edit`                                  | Both   | as `custom`, answering `ExtendBuiltIn { actions: [act-pin], hidden: [Edit] }`                                                                                                                                                          | The built-in entries without Edit, then Pin                                                                                 | S25           |
| `/menu none`                                       | Both   | `set_room_config` with `message_actions: false`                                                                                                                                                                                      | The host's default menu, with no query                                                                                       | S25           |
| `/toolbar <n>`                                     | Both   | `set_room_toolbar` with `tb-1` to `tb-<n>`, icons `icon-<i>.png`; `tb-1` is `Deeplink` to the App with `?from=deeplink`, the rest `Callback`; `/toolbar 0` sends an empty list                                                         | Two buttons inline, the rest in an overflow menu; a callback press sends `ToolbarActionTriggered`                           | S29           |
| `/participants <n>`                                | Both   | `set_room_config` with `participants: true` and `participant_actions: true`; `participants_query` answers `Count { 4 + n }`, pages of up to 100 in id order, and `ByIds` for any of them; `participant_actions_query` answers `ExtendBuiltIn { [act-mention] }` | "4 + n participants" in the header; the list pages; each kind draws its own appearance; the sheet shows the kind's entry and Mention | S30, S31, B1 |
| `/reactions on`, `na`, `off`                       | Both   | `set_room_config` with `reaction_details: true`, `true` or `false`; `reactions_query` answers every reaction with its participant for `on`, interrupts `NotAvailable` for `na`                                                         | "Who reacted" lists `p1` and the user; `na` and `off` hide it                                                              | S32           |
| `/footer input`                                    | Both   | `set_room_footer` with `Input { placeholder: "Message the sample", photo: false, video: false, file: false, payment: None }`                                                                                                          | The input shows the hint and no attachment button                                                                            | S33           |
| `/footer photo`                                    | Both   | `set_room_footer` with `Input` and `photo`, `video`, `file` all `true`                                                                                                                                                                | Three attachment buttons; an attachment under 1 MiB arrives inline                                                           | S33, S36      |
| `/footer empty`                                    | Both   | `set_room_footer` with `Empty`                                                                                                                                                                                                       | No input; the messages fill the screen                                                                                      | S34           |
| `/footer custom`                                   | Both   | `set_room_footer` with `Custom`; `Renderer::render` with `ChatFooter { room_id }` answers a line of text and a `footer-ping` button, whose press posts "footer ping"                                                                   | The product's tree in the footer slot; a press reaches `Renderer::action_subscribe`                                          | S35           |
| `/footer pay`                                      | Both   | `set_room_footer` with the default `Input` and `payment: Some(Enabled { destination: 0x05 repeated 32 times })`                                                                                                                        | A payment button where the host has a payment path, none elsewhere                                                          | S33           |
| `/placeholder <text>`                              | Both   | `set_room_config` with `placeholder: Some(<text>)` on `host` and `source`; creates the host-store room `host-empty` with the same placeholder if it does not exist; `/placeholder` alone sets `None`                                    | `<text>` in an empty `source` (after `/history 0`) and in `host-empty`                                                     | S37           |
| `/suggest`, `/suggest off`                         | Both   | `set_room_suggestions` with `sg-send "Say hello"` (`sends_text: true`) and `sg-event "Show help"` (`sends_text: false`), or an empty list; `SuggestionSelected` for `sg-event` runs `/help`                                            | Two chips above the input, hidden while typing; one sends "Say hello", the other sends an event                             | S38           |
| `/commands`, `/commands off`                       | Both   | `set_room_commands` with every command of this table, its description and argument hint, or an empty list                                                                                                                             | Typing `/` opens the catalogue; a sent command arrives as `Command` and posts no message                                    | S14, S39      |
| `/activity on`, `off`                              | Both   | `set_room_activity` with `Some({ text: "Sample is typing", participant_id: None })` or `None`                                                                                                                                          | The line with its animation above the input; it clears on `off` and on `RoomClosed`                                         | S40           |
| `/allowed <emoji>`                                 | Both   | `set_room_config` with `allowed_reactions: Some([...])`, one entry per grapheme; `/allowed` alone sets `None`                                                                                                                          | The picker offers only those emoji                                                                                          | S41           |
| `/history <n>`                                     | `source` | Replaces the stored history with `n` generated messages: every fifth by `p1` to `p4` in turn, the rest by `"product"`, every 25th a reply to the one 10 before it, every 50th with a reaction of `p1`; `messages_query` pages from it with the cursor `<index>` | After reopening, the newest page at the bottom; older pages on scroll until the oldest, none asked twice; a reply quote jumps across pages | S15 to S17, S22, B2 |
| `/slow <ms>`                                       | Both   | No call; every later query and `user_action` answer waits `ms` first; `/slow 0` ends it                                                                                                                                              | Loading rows, then the answer; past the wait of the RFC, the host's time-out behaviour                                     | S21, S46      |
| `/fail reject`                                     | `source` | The next `user_action` `NewMessage` answers `Rejected { reason: "Rejected by /fail" }`                                                                                                                                                            | A sent message is removed and the reason shown                                                   | S20           |
| `/fail timeout`                                    | `source` | The next `user_action` `NewMessage` gets no answer                                                                                                                                                                                                | After 30 s the message is `Failed`; retry sends `Retry`, answered `Acknowledged` with `StatusChanged { Sent }`              | S21           |
| `/fail crash`                                      | `source` | The next `user_action` `NewMessage` handler throws outside any promise, which ends the worker                                                                                                                                                     | Cached pages stay, the input says the product is unavailable, retry starts the worker again                                 | S23           |
| `/fail unavailable`                                | Both   | The next query interrupts with `NotAvailable`                                                                                                                                                                                        | The host default for that item, with no error shown                                                                         | S32, S46      |
| `/deeplink`                                        | Both   | `set_room_config` with `message_actions: true`; posts a `Text` message whose `message_actions_query` answer is `Custom { [act-open-app] }` with `Deeplink { url: "polkadot://<product>/?from=deeplink" }`                              | A long press offers "Open the App"; a pick asks for confirmation and opens the App with its banner                          | S27           |
| `/echo`                                            | Both   | Toggles echo. While on, every user event in the room is answered with `update_feed` `New` `Text "echo <event> <summary>"`                                                                                                             | One echo per send, reply, edit, delete, reaction and seen report                                                            | S12 to S14    |

Events and queries the product answers outside the table:

| Received                                                     | Product answer                                                                                    |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| `user_action` `NewMessage` in `source`                       | Stores the message from `"user"` with status `Sent`, answers `MessageCreated`, then runs a command |
| `user_action` `EditMessage`, `DeleteMessage`, reactions      | Applies them to the store and answers `Acknowledged`                                              |
| `user_action` `Seen` in `source`                             | `set_room_preview` with `badge: Some(UnreadCount { count: 0 })`, then `Acknowledged`               |
| `user_action` `Retry`                                        | `update_feed` `StatusChanged { Sent }`, then `Acknowledged`                                       |
| `MessageActionTriggered`, `ParticipantActionTriggered`, `ToolbarActionTriggered` | `update_feed` `New` `Text "<event> <action_id> on <target>"`                   |
| `RoomOpened`                                                 | Restores the activity line when `/activity on` is set                                             |
| `RoomClosed`, `UserEdited`, `UserDeleted`, `UserReacted`, `Seen` in `host` | Logged; echoed when `/echo` is on                                                   |
| `Renderer::action_subscribe` for `card-like` or `footer-ping` | `update_feed` `New` `Text "card liked"` or `"footer ping"`                                       |

## Event log

The Worker writes one JSON object per line with `console.info` for every event, query and render request it receives,
and one more line for every query it answers. A collector keeps the lines whose `t` is `"chat-v2"`.

```json
{"t":"chat-v2","boot":1,"seq":14,"via":"user_action","room":"source","kind":"NewMessage","data":{"body":{"tag":"Text","value":{"text":"/help"}},"replyTo":null}}
{"t":"chat-v2","boot":1,"seq":15,"via":"answer","re":14,"room":"source","kind":"MessageCreated","data":{"messageId":"source-u-0003"}}
{"t":"chat-v2","boot":1,"seq":16,"via":"action_subscribe","room":"host","kind":"ActionTriggered","data":{"messageId":"host-p-0007","actionId":"act-yes","payload":null}}
```

| Field  | Value                                                                                                                                     |
| ------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `t`    | Always `"chat-v2"`                                                                                                                        |
| `boot` | How many times the Worker started, kept in product storage; a worker restart shows as a new `boot`                                      |
| `seq`  | Line counter of this boot, from 1                                                                                                         |
| `via`  | `action_subscribe`, one of the six query names, `render`, `renderer_action`, or `answer`                                                  |
| `re`   | For `answer`: the `seq` of the query it answers                                                                                           |
| `room` | The room id, or `null` for a render request without one                                                                                   |
| `kind` | The event, action, query or answer variant: `MessagePosted`, `NewMessage`, `Before`, `Count`, `MessageCreated`, `NotAvailable`, `Timeout` |
| `data` | The payload as the TypeScript client decodes it, with a `bigint` as a decimal string and bytes as `0x` hex                                |

An answer the product never sends, under `/fail timeout`, logs `kind: "Timeout"` when the host stops the stream. The
log carries no wall-clock time, so two runs of one command list produce the same lines except for host-assigned ids.

## Running it

Build first, from the repository root, after the `@parity/truapi` build:

```bash
cd playground/chat-v2
yarn install --frozen-lockfile
yarn build             # out/ and out/worker/index.js
```

| Where          | Available after | How                                                                                                                                                                                                                                                         |
| -------------- | --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Test host      | P2              | `make wasm` and `npm run build` in `js/packages/truapi-host`, then `REQUIRE_WASM=1 yarn test`. `tests/e2e/` runs the Worker against `createMockClient` and `createMockHost` from `@parity/truapi-host/testing`, sends each command, and asserts the calls the host received, the queries answered, and the event log |
| CLI chat page  | P2              | `truapi-host dev --worker-bundle out/worker/index.js -- yarn dev`, then open the `/chat` URL it prints and type the commands; the worker log pane shows the event log. `make e2e-chat-cli` runs the command list without a browser                         |
| iOS simulator  | P3; every command after P6 | `make ios-chat-v2-run` ([runbook](../../docs/chat-ios-simulator.md)). The launcher builds the product, seeds it, runs the command list of each scenario the app serves, takes a screenshot per scenario and writes the diagnosis report; the other scenarios are `skip` rows that name their phase |
| Android emulator | P8            | `make android-chat-playground-run` with `TRUAPI_ANDROID_E2E_CHAT_PRODUCT_DIR=playground/chat-v2`, on a debug build whose chat products run on the core runtime                                                                                                |

On every host, the diagnosis report has one row per scenario the command list drove, `pass`, `fail` with the first
failing assertion, or `skip` with a reason. A row passes when the host's screen matches the expected behaviour of the
command table and the event log matches the test host's log for the same commands.
