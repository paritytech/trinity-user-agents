// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The scenarios of the Chat Extension v2 test plan (04-TESTING section 4)
 * that name the iOS layer, in the order the iOS launcher runs them.
 *
 * A scenario with `drive` runs on the app: the launcher goes through its
 * `phases` in order, sends the `steps` of a phase, checks its `expect`, and
 * takes a screenshot at the end. `after` puts the room back for the next
 * scenario. A scenario with `skip` names why the launcher cannot drive it.
 *
 * A step is one of:
 * - a string: a command to the sample product, in the room of the scenario;
 * - `{ text, room }`: a command in another room;
 * - `{ host, argument, target, room }`: an action of the user on the room
 *   screen, which the app's simulator-only harness runs (`open`, `list`,
 *   `send`, `menu`, `toolbar`, `confirm`, `participants`, `participant`,
 *   `reactions`, `suggestion`, `type`, `attach`, `button`, `footer-action`,
 *   `older`, `jump`, `retry`, `retry-room`, `back`);
 * - `{ wait }`: a pause in milliseconds.
 *
 * An expectation is one of:
 * - `{ stored }`: a text message the room stores (host-store rooms);
 * - `{ storedMatch }`: a stored text message that matches the pattern;
 * - `{ metadata }`: columns of the room's `CDChatRoomMetadata` row;
 * - `{ gone: true }`: the app stores no chat for the room;
 * - `{ contentType, added }`: at least `added` more messages of this content
 *   kind in the room than when the scenario started;
 * - `{ reactions }`: the change of the reaction rows of the room in the phase;
 * - `{ ui, match }`: a `[e2e:ui] <ui> <room> …` console marker of the app
 *   that matches, written in the phase;
 * - `{ uiNot, match }`: the newest marker of a key does not match;
 * - `{ event }`: an event log entry of the product written in the phase,
 *   with `via`, `kind`, and an optional `data` substring;
 * - `{ console }`: an app console line written in the phase.
 *
 * `scope: "previous"` makes a `console` or `event` expectation read from the
 * start of the previous phase instead of the start of its own phase.
 *
 * `{lastUser}` in a step or an expectation is the newest message of the
 * user in the room's window, as the app's `window` marker names it.
 * `{lastProduct}` is the newest message of the product, and
 * `{memory:<name>}` a value an earlier step filled in. `{reacted}` is the
 * product id of the newest message with a reaction, and
 * `{file:<name>}` a file the launcher put in the app's tmp directory.
 */
const SOURCE = "source";

export const IOS_CHAT_V2_SCENARIOS = Object.freeze([
  {
    id: "S2",
    title: "create_room V2 makes the host-store room",
    mode: "host",
    phase: "P3",
    drive: {
      phases: [
        {
          steps: [],
          expect: [{ metadata: { ZNAME: "Sample: host store" } }],
        },
      ],
    },
  },
  {
    id: "S1",
    title: "post_message V2 posts a product message in a host-store room",
    mode: "host",
    phase: "P3",
    drive: {
      phases: [
        {
          steps: ["/post legacy"],
          expect: [{ stored: "Legacy message from post_message" }],
        },
      ],
    },
  },
  {
    id: "S3",
    title: "set_room_metadata updates the list row and the header",
    mode: "host",
    phase: "P3",
    drive: {
      phases: [
        {
          steps: ["/meta Renamed"],
          expect: [
            { stored: "ok /meta Renamed" },
            {
              metadata: {
                ZNAME: "Renamed",
                ZROOMDESCRIPTION: "Renamed by /meta",
              },
            },
          ],
        },
      ],
      after: ["/meta Sample: host store"],
    },
  },
  {
    id: "S4",
    title: "every preview variant in the chat list",
    mode: "host",
    phase: "P3",
    drive: {
      phases: [
        {
          steps: [
            "/preview pin",
            "/preview badge 3",
            "/preview text Hello from the preview",
            { host: "list" },
            { wait: 1_000 },
          ],
          expect: [
            {
              metadata: {
                ZPREVIEWORDER: "1",
                ZBADGECOUNT: "3",
                ZPREVIEWTEXT: "Hello from the preview",
              },
            },
          ],
        },
      ],
      after: [
        "/preview unpin",
        "/preview typing off",
        "/preview none",
        { host: "open" },
      ],
    },
  },
  {
    id: "S5",
    title: "delete_room removes the room",
    mode: "host",
    phase: "P3",
    drive: {
      phases: [
        {
          steps: ["/room create"],
          expect: [
            {
              metadata: { ZNAME: "Sample: throwaway" },
              room: "throwaway",
            },
          ],
        },
        {
          // The screen of the room is open while the product deletes it.
          steps: [
            { host: "open", room: "throwaway" },
            { text: "/room delete", room: "host" },
          ],
          expect: [
            { event: { via: "room", kind: "Deleted", room: "throwaway" } },
            { gone: true, room: "throwaway" },
            { ui: "screen", room: "throwaway", match: "^removed$" },
          ],
        },
      ],
      after: [{ host: "list" }, { host: "open" }],
    },
  },
  {
    id: "S6",
    title: "the chat list renders product rooms with the worker stopped",
    mode: "host",
    phase: "P3",
    drive: {
      phases: [
        {
          // The list screen holds no worker, and the harness gives back its
          // own hold, so the core stops the worker.
          steps: [{ host: "list" }, { host: "release-worker" }],
          expect: [
            {
              console:
                "[truapi] worker stopped: polkadot://truapi-chat-v2.dot/",
              timeoutMs: 30_000,
            },
          ],
        },
        {
          // The list draws again from what the app stores, with no worker.
          steps: [{ host: "list-render" }, { wait: 1_000 }],
          expect: [
            {
              console: "[e2e:ui] chat-list list rooms=",
            },
            { ui: "chat-list", room: "list", match: "Sample: product source" },
            {
              uiNot: "chat-list",
              room: "list",
              match: "^rooms=$",
              settleMs: 500,
            },
          ],
          shot: "stopped",
        },
      ],
      // The harness holds the worker again for the next scenarios.
      after: [{ host: "hold-worker" }, { wait: 4_000 }, { host: "open" }],
    },
  },
  {
    id: "S7",
    title: "New renders every body kind",
    mode: "host",
    phase: "P4",
    drive: {
      phases: [
        {
          steps: ["/post all"],
          expect: [
            { stored: "Plain text", prefix: true },
            { contentType: 247, added: 4 },
            { contentType: 252, added: 1 },
          ],
        },
      ],
    },
  },
  {
    id: "S8",
    title: "Edited replaces in place",
    mode: "host",
    phase: "P4",
    drive: {
      phases: [
        {
          steps: ["/post text", "/edit last Edited by the iOS launcher"],
          expect: [
            { stored: "Edited by the iOS launcher" },
            { stored: "ok /edit last Edited by the iOS launcher" },
          ],
        },
      ],
    },
  },
  {
    id: "S9",
    title: "Deleted tombstones",
    mode: "host",
    phase: "P4",
    drive: {
      phases: [
        {
          steps: ["/post text", "/delete last"],
          expect: [
            { stored: "ok /delete last" },
            { contentType: 246, added: 1 },
          ],
        },
      ],
    },
  },
  {
    id: "S10",
    title: "reactions aggregate",
    mode: "host",
    phase: "P4",
    drive: {
      phases: [
        {
          steps: ["/post text", "/react last 👍"],
          expect: [{ stored: "ok /react last 👍" }, { reactions: 1 }],
          shot: "placed",
        },
        {
          steps: ["/unreact {reacted} 👍"],
          expect: [{ stored: "ok /unreact", prefix: true }, { reactions: -1 }],
        },
      ],
    },
  },
  {
    id: "S12",
    title: "user events reach the product",
    mode: "host",
    phase: "P4",
    drive: {
      phases: [
        {
          steps: ["/echo", { host: "send", argument: "hello from the user" }],
          expect: [
            {
              event: {
                via: "action_subscribe",
                kind: "MessagePosted",
                data: "hello from the user",
                hostId: true,
              },
            },
            { stored: "echo MessagePosted", prefix: true },
          ],
        },
      ],
      after: ["/echo"],
    },
  },
  {
    id: "S13",
    title: "an Actions press sends ActionTriggered",
    mode: "host",
    phase: "P4",
    drive: {
      phases: [
        {
          steps: [
            "/post actions",
            { host: "button", target: "last", argument: "act-yes" },
          ],
          expect: [
            {
              event: {
                via: "action_subscribe",
                kind: "ActionTriggered",
                data: "act-yes",
              },
            },
          ],
        },
      ],
    },
  },
  {
    id: "S14",
    title: "a command arrives as Command",
    mode: "host",
    phase: "P4",
    drive: {
      phases: [
        {
          steps: ["/commands", { host: "send", argument: "/help" }],
          expect: [
            {
              event: { via: "action_subscribe", kind: "Command", data: "help" },
            },
          ],
        },
      ],
      after: ["/commands off"],
    },
  },
  {
    id: "S25",
    title: "ExtendBuiltIn menu",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: [
            "/menu extend",
            { host: "send", argument: "a message to edit" },
            { wait: 1_000 },
            { host: "menu", target: "lastUser" },
          ],
          expect: [{ ui: "menu", match: "entries=.*edit.*product:act-pin" }],
        },
        {
          steps: ["/menu hide-edit", { host: "menu", target: "lastUser" }],
          expect: [
            { ui: "menu", match: "entries=(?!.*\\bedit\\b).*product:act-pin" },
          ],
        },
      ],
      after: ["/menu none"],
    },
  },
  {
    id: "S26",
    title: "Custom menu",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: [
            "/menu custom",
            "/post text",
            { host: "menu", target: "last" },
          ],
          expect: [
            {
              ui: "menu",
              match: "entries=product:act-pin,product:act-report!$",
            },
          ],
        },
        {
          steps: ["/slow 6000", { host: "menu", target: "last" }],
          expect: [
            {
              ui: "menu",
              match: "entries=(?!.*product:).*copy",
              timeoutMs: 20_000,
            },
          ],
        },
      ],
      after: ["/slow 0", "/menu none"],
    },
  },
  {
    id: "S28",
    title: "callback action",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: [
            "/menu custom",
            "/post text",
            { host: "menu", target: "last", argument: "act-pin" },
          ],
          expect: [
            {
              event: {
                via: "action_subscribe",
                kind: "MessageActionTriggered",
                data: "act-pin",
              },
            },
            { stored: "MessageActionTriggered act-pin", prefix: true },
          ],
        },
      ],
      after: ["/menu none"],
    },
  },
  {
    id: "S27",
    title: "deeplink action",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: [
            "/deeplink",
            { host: "menu", target: "last", argument: "act-open-app" },
          ],
          expect: [{ console: "[e2e:ui] confirm polkadot://" }],
          shot: "confirm",
        },
        {
          steps: [{ host: "confirm" }],
          expect: [{ console: "Product link opens: polkadot://" }],
        },
      ],
      after: [{ host: "open" }, "/menu none"],
    },
  },
  {
    id: "S29",
    title: "toolbar",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: ["/toolbar 4", { wait: 1_000 }],
          expect: [
            {
              ui: "toolbar",
              match: "inline=tb-1,tb-2 overflow=tb-3,tb-4",
            },
          ],
          shot: "bar",
        },
        {
          steps: [{ host: "toolbar", argument: "tb-2" }],
          expect: [
            {
              event: {
                via: "action_subscribe",
                kind: "ToolbarActionTriggered",
                data: "tb-2",
              },
            },
          ],
        },
      ],
      after: ["/toolbar 0"],
    },
  },
  {
    id: "S33",
    title: "footer Input honours placeholder and attachment toggles",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: ["/footer input"],
          expect: [
            {
              ui: "footer",
              match: "kind=input placeholder=Message the sample attach=$",
            },
          ],
          shot: "input",
        },
        {
          steps: ["/footer photo"],
          expect: [{ ui: "footer", match: "attach=photo,video,file$" }],
        },
      ],
    },
  },
  {
    id: "S36",
    title: "inline attachment under 1 MiB",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: [
            { host: "attach", argument: "{file:sample.png}", target: "image" },
          ],
          expect: [
            {
              event: {
                via: "action_subscribe",
                kind: "MessagePosted",
                data: '"tag":"Image"',
              },
            },
            { contentType: 247, added: 1 },
          ],
        },
        {
          steps: [
            { host: "attach", argument: "{file:large.bin}", target: "file" },
          ],
          expect: [{ ui: "failure", match: "attachment too large" }],
        },
      ],
      after: ["/footer input"],
    },
  },
  {
    id: "S34",
    title: "footer Empty hides the input",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: ["/footer empty"],
          expect: [
            { console: "[truapi:chat-bridge] setRoomFooter host" },
            { ui: "footer", match: "kind=empty" },
          ],
        },
      ],
      after: ["/footer input"],
    },
  },
  {
    id: "S35",
    title: "footer Custom",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: ["/footer custom", { wait: 1_500 }],
          expect: [
            { ui: "footer", match: "kind=custom" },
            { event: { via: "render", kind: "ChatFooter" } },
          ],
          shot: "drawn",
        },
        {
          steps: [{ host: "footer-action", argument: "footer-ping" }],
          expect: [
            { event: { via: "renderer_action", data: "footer-ping" } },
            { stored: "footer ping" },
          ],
        },
      ],
      after: ["/footer input"],
    },
  },
  {
    id: "S37",
    title: "placeholder on an empty room",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: [
            "/placeholder Nothing here yet",
            { wait: 1_000 },
            { host: "open", room: "host-empty" },
          ],
          expect: [
            {
              ui: "placeholder",
              room: "host-empty",
              match: "Nothing here yet",
            },
          ],
        },
      ],
      after: [{ host: "open" }, "/placeholder"],
    },
  },
  {
    id: "S38",
    title: "suggestion chips",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: ["/suggest"],
          expect: [{ ui: "suggestions", match: "sg-send,sg-event" }],
          shot: "chips",
        },
        {
          steps: [{ host: "suggestion", argument: "sg-event" }],
          expect: [
            {
              event: {
                via: "action_subscribe",
                kind: "SuggestionSelected",
                data: "sg-event",
              },
            },
          ],
        },
        {
          steps: [{ host: "suggestion", argument: "sg-send" }],
          expect: [
            {
              event: {
                via: "action_subscribe",
                kind: "MessagePosted",
                data: "Say hello",
                hostId: true,
              },
            },
          ],
        },
      ],
      after: ["/suggest off"],
    },
  },
  {
    id: "S39",
    title: "the / catalogue",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: ["/commands", { host: "type", argument: "/h" }, { wait: 800 }],
          expect: [{ ui: "catalogue", match: "^help,history$" }],
          shot: "catalogue",
        },
        {
          steps: [
            { host: "type", argument: "" },
            { host: "send", argument: "/help" },
          ],
          expect: [
            {
              event: { via: "action_subscribe", kind: "Command", data: "help" },
            },
          ],
        },
      ],
      after: ["/commands off"],
    },
  },
  {
    id: "S40",
    title: "activity line",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: ["/activity on"],
          expect: [{ ui: "activity", match: "Sample is typing" }],
          shot: "line",
        },
        {
          steps: ["/activity off"],
          expect: [{ ui: "activity", match: "^none$" }],
        },
        {
          steps: ["/activity on", { host: "list" }, { wait: 1_000 }],
          expect: [{ event: { via: "action_subscribe", kind: "RoomClosed" } }],
        },
      ],
      after: [{ host: "open" }, "/activity off"],
    },
  },
  {
    id: "S41",
    title: "allowed_reactions",
    mode: "host",
    phase: "P6",
    drive: {
      phases: [
        {
          steps: ["/allowed 👍❤️"],
          expect: [{ ui: "allowed", match: "^👍❤️$" }],
        },
      ],
      after: ["/allowed"],
    },
  },
  {
    id: "S44",
    title: "update_feed rate limit on screen",
    mode: "host",
    phase: "P4",
    drive: {
      phases: [
        {
          // `/flood` sends 25 edits of one message at once. The core refuses
          // the edits past the budget of 20, and the room screen draws at
          // most one list update in each frame.
          steps: [
            { host: "list" },
            { host: "open" },
            { wait: 1_000 },
            { host: "frames", argument: "5" },
            "/flood",
          ],
          expect: [
            { event: { via: "flood", kind: "Done" } },
            { storedMatch: "^ok /flood accepted=\\d+ refused=[1-9]" },
            { stored: "Flood of 25 edits" },
            { console: "[e2e:ui] frames host frames=", timeoutMs: 15_000 },
          ],
        },
      ],
    },
  },
  {
    id: "S24",
    title: "RoomOpened and RoomClosed",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: [{ host: "list" }, { host: "open" }],
          expect: [{ event: { via: "action_subscribe", kind: "RoomOpened" } }],
        },
        {
          steps: [{ host: "list" }],
          expect: [{ event: { via: "action_subscribe", kind: "RoomClosed" } }],
        },
      ],
      after: [{ host: "open" }],
    },
  },
  {
    id: "S15",
    title: "product-source room opens on Before { None }",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: ["/history 120", { host: "list" }, { host: "open" }],
          expect: [
            { event: { via: "messages_query", kind: "Before", data: "null" } },
            { ui: "window", match: "source-h-00120:" },
          ],
        },
      ],
    },
  },
  {
    id: "S16",
    title: "scroll pages with the cursor",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: [{ host: "older" }],
          expect: [
            { event: { via: "messages_query", kind: "Before", data: '"70"' } },
            { ui: "window", match: "count=100 " },
          ],
        },
      ],
    },
  },
  {
    id: "S17",
    title: "a reply quote jumps across pages",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: [{ host: "jump", target: "source-h-00005" }],
          expect: [{ ui: "readiness", match: "older=end", timeoutMs: 30_000 }],
        },
      ],
    },
  },
  {
    id: "S18",
    title: "feed updates apply to the window",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: [{ host: "list" }, { host: "open" }, "/post text"],
          expect: [{ ui: "window", match: "source-p-\\d+:in-[a-z]+:text$" }],
        },
        {
          steps: ["/delete {lastProduct}"],
          expect: [
            {
              event: {
                via: "action_subscribe",
                kind: "MessagePosted",
                data: "/delete",
              },
            },
            {
              uiNot: "window",
              match: "{memory:lastProduct}:",
              settleMs: 2_000,
            },
          ],
        },
      ],
    },
  },
  {
    id: "S19",
    title: "send shows Preparing, then MessageCreated",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: [{ host: "send", argument: "hello source" }],
          expect: [
            {
              event: {
                via: "user_action",
                kind: "NewMessage",
                data: "hello source",
              },
            },
            { event: { via: "answer", kind: "MessageCreated" } },
            { ui: "window", match: "source-u-\\d+:out-sent:text" },
          ],
        },
      ],
    },
  },
  {
    id: "S11",
    title: "StatusChanged marks",
    mode: SOURCE,
    phase: "P4",
    drive: {
      phases: [
        {
          steps: ["/status {lastUser} read"],
          expect: [{ ui: "window", match: "{lastUser}:out-read" }],
        },
        {
          steps: [
            "/status {lastUser} failed",
            { host: "menu", target: "lastUser" },
          ],
          expect: [
            { ui: "window", match: "{lastUser}:out-failed" },
            { ui: "menu", match: "entries=.*retry" },
          ],
        },
      ],
    },
  },
  {
    id: "S20",
    title: "Rejected reverts the send",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: ["/fail reject", { host: "send", argument: "to be rejected" }],
          expect: [
            {
              event: {
                via: "user_action",
                kind: "NewMessage",
                data: "to be rejected",
              },
            },
            { event: { via: "answer", kind: "Rejected" } },
            { uiNot: "window", match: "local:", settleMs: 2_000 },
          ],
        },
      ],
    },
  },
  {
    id: "S21",
    title: "no answer in 30 s marks Failed",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: ["/fail timeout", { host: "send", argument: "no answer" }],
          expect: [
            {
              ui: "window",
              match: "local:[^:]+:out-failed:text$",
              timeoutMs: 45_000,
            },
          ],
          shot: "failed",
        },
      ],
    },
  },
  {
    id: "S30",
    title: "participants",
    mode: SOURCE,
    phase: "P6",
    drive: {
      phases: [
        {
          steps: ["/participants 6", { wait: 1_000 }],
          expect: [
            { ui: "header", match: "^10 participants$" },
            { ui: "author-p1", match: "Person #1" },
          ],
          shot: "header",
        },
        {
          steps: [{ host: "participants" }, { wait: 1_500 }],
          expect: [{ console: "[e2e:ui] participants-list rows=10" }],
        },
      ],
      after: [{ host: "list" }, { host: "open" }],
    },
  },
  {
    id: "S31",
    title: "participant sheet",
    mode: SOURCE,
    phase: "P6",
    drive: {
      phases: [
        {
          steps: [{ host: "participant", target: "p4" }, { wait: 1_000 }],
          expect: [
            {
              ui: "sheet",
              match: "^p4 entries=openProduct,product:act-mention$",
            },
          ],
          shot: "sheet",
        },
        {
          steps: [
            { host: "list" },
            { host: "open" },
            { host: "participant", target: "p1", argument: "act-mention" },
          ],
          expect: [
            {
              event: {
                via: "action_subscribe",
                kind: "ParticipantActionTriggered",
                data: "act-mention",
              },
            },
          ],
        },
      ],
    },
  },
  {
    id: "S32",
    title: "who reacted",
    mode: SOURCE,
    phase: "P6",
    drive: {
      phases: [
        {
          steps: [
            "/reactions on",
            "/post text",
            "/react {lastProduct} 👍",
            { host: "reactions", target: "{memory:lastProduct}" },
          ],
          expect: [
            { event: { via: "reactions_query" } },
            { ui: "reactions", match: "👍:Person #1" },
          ],
          shot: "list",
        },
        {
          steps: [
            "/reactions na",
            { host: "list" },
            { host: "open" },
            { host: "reactions", target: "{memory:lastProduct}" },
          ],
          expect: [{ ui: "reactions", match: "^unavailable$" }],
        },
      ],
      after: ["/reactions off", { host: "list" }, { host: "open" }],
    },
  },
  {
    id: "S46",
    title: "NotAvailable retry then default",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: [
            "/menu custom",
            "/post text",
            "/fail unavailable",
            { host: "menu", target: "last" },
          ],
          expect: [{ ui: "menu", match: "entries=(?!.*product:)" }],
        },
      ],
      after: ["/menu none"],
    },
  },
  {
    id: "S22",
    title: "the window stays under 500 messages",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: [
            "/history 700",
            { host: "list" },
            { host: "open" },
            { wait: 2_000 },
            ...Array.from({ length: 13 }, () => [
              { host: "older" },
              { wait: 700 },
            ]).flat(),
          ],
          expect: [
            { ui: "window", match: "count=(500|[1-4]\\d\\d|\\d\\d) " },
            {
              uiNot: "window",
              match: "count=(50[1-9]|5[1-9]\\d|[6-9]\\d\\d) ",
              settleMs: 1_000,
            },
          ],
        },
      ],
    },
  },
  {
    id: "S23",
    title: "a killed worker keeps the cached pages",
    mode: SOURCE,
    phase: "P5",
    drive: {
      phases: [
        {
          steps: [{ host: "list" }, { host: "open" }],
          expect: [{ ui: "readiness", match: "^ready " }],
        },
        {
          // The send makes the product throw outside any promise. The
          // uncaught error ends the worker under the open room.
          steps: [
            "/post text",
            "/fail crash",
            { host: "send", argument: "crash the worker" },
          ],
          expect: [
            { console: "worker ended on an uncaught error" },
            { ui: "readiness", match: "^unavailable ", timeoutMs: 45_000 },
            { ui: "window", match: "count=[1-9]" },
          ],
          shot: "unavailable",
        },
        {
          // Retry asks for the newest page. The first request after the
          // fault starts the worker again: the retry, or a `Seen` report of
          // the screen that came first, so the checks read from the crash on.
          steps: [{ host: "retry-room" }],
          expect: [
            {
              console: "worker starts again after its fault",
              scope: "previous",
            },
            {
              event: { via: "action_subscribe", kind: "RoomOpened" },
              scope: "previous",
            },
            { ui: "readiness", match: "^ready ", timeoutMs: 45_000 },
          ],
        },
      ],
    },
  },
  {
    id: "S47",
    title: "configuration generation reload",
    mode: "host",
    phase: "P7",
    drive: {
      // The first phase opens the room screen, and then no step opens it
      // again. Each `set_room_config` writes a new generation, and the open
      // screen draws the new policy.
      phases: [
        {
          steps: [{ host: "list" }, { host: "open" }, "/allowed 👍"],
          expect: [
            { console: "[e2e:ui] policy host generation=" },
            { ui: "allowed", match: "^👍$" },
          ],
        },
        {
          steps: ["/allowed"],
          expect: [
            { console: "[e2e:ui] policy host generation=" },
            { ui: "allowed", match: "^all$" },
          ],
        },
      ],
      after: ["/allowed"],
    },
  },
]);

/**
 * The performance rows of 04-TESTING section 5 (P7.1). The launcher drives
 * them after the scenarios when `TRUAPI_IOS_E2E_PERF=1`. Each row writes its
 * numbers in its evidence: the `perf-participants`, `perf-ready` and
 * `frames` markers of the app's simulator-only frame meter
 * (`TrUAPIE2EFrameMeter`), where a dropped frame is a frame time of 1.5
 * frames or more.
 */
export const IOS_CHAT_V2_PERF = Object.freeze([
  {
    id: "B1",
    title: "10,000 participants paged at 100, scrolled to the end",
    mode: SOURCE,
    phase: "P7",
    perf: true,
    drive: {
      phases: [
        {
          steps: [
            "/participants 10000",
            { host: "list" },
            { host: "open" },
            { wait: 1_500 },
            { host: "participants" },
            { wait: 2_000 },
            {
              host: "participants-scroll",
              argument: "10004",
              target: "300",
            },
          ],
          expect: [
            {
              ui: "perf-participants",
              match: "rows=10004 ",
              timeoutMs: 150_000,
            },
          ],
        },
      ],
      after: ["/participants 6", { host: "list" }, { host: "open" }],
    },
  },
  {
    id: "B2",
    title: "50,000-message product-source room, newest page",
    mode: SOURCE,
    phase: "P7",
    perf: true,
    drive: {
      phases: [
        {
          // Warm: the harness holds the worker, so the open asks a running
          // product for its newest page.
          steps: [
            "/history 50000",
            { host: "list" },
            { wait: 1_000 },
            { host: "open" },
          ],
          expect: [
            { ui: "perf-ready", match: "^ms=\\d+ ", timeoutMs: 30_000 },
            { ui: "window", match: "source-h-50000:" },
          ],
          shot: "warm",
        },
        {
          // Cold: an uncaught error ends the worker, and the retry starts a
          // new worker that answers the newest page.
          steps: [
            "/fail crash",
            { host: "send", argument: "end the worker" },
            { wait: 3_000 },
            { host: "retry-room" },
          ],
          expect: [
            { console: "worker starts again after its fault" },
            { console: "[e2e:ui] perf-ready source ms=", timeoutMs: 45_000 },
          ],
        },
      ],
      after: ["/history 120", { host: "list" }, { host: "open" }],
    },
  },
  {
    id: "B3",
    title: "/stream at 20 edits a second, and /flood",
    mode: "host",
    phase: "P7",
    perf: true,
    drive: {
      phases: [
        {
          // The baseline: the open room with nothing to draw.
          steps: [
            { host: "list" },
            { host: "open" },
            { wait: 1_000 },
            { host: "frames", argument: "4" },
          ],
          expect: [
            { console: "[e2e:ui] frames host frames=", timeoutMs: 15_000 },
          ],
        },
        {
          steps: [{ host: "frames", argument: "4" }, "/stream"],
          expect: [
            { console: "[e2e:ui] frames host frames=", timeoutMs: 15_000 },
          ],
        },
        {
          steps: [{ host: "frames", argument: "5" }, "/flood"],
          expect: [
            { event: { via: "flood", kind: "Done" } },
            { console: "[e2e:ui] frames host frames=", timeoutMs: 15_000 },
          ],
        },
      ],
    },
  },
]);

/** The prefix the app's logger puts before each console line of a product. */
const JS_LINE = /JS: (\{"t":"chat-v2".*\})\s*$/;

/** The marker lines of the app's simulator-only harness. */
const UI_LINE = /\[e2e:ui\] (\S+) (\S+) (.*)$/;

/**
 * The event log lines of the sample product in the app's console output, as
 * objects. The launcher makes the Worker's `console.info` reach the logger.
 */
export function parseEventLog(text) {
  const entries = [];
  for (const line of text.split(/\r?\n/)) {
    const match = line.match(JS_LINE);
    if (!match) continue;
    try {
      entries.push(JSON.parse(match[1]));
    } catch {
      // A line the logger cut is not an entry.
    }
  }
  return entries;
}

/**
 * The `[e2e:ui]` markers of the app in its console output, in order, as
 * `{ key, room, value }`.
 */
export function parseUiMarkers(text) {
  const markers = [];
  for (const line of text.split(/\r?\n/)) {
    const match = line.match(UI_LINE);
    if (match) {
      markers.push({ key: match[1], room: match[2], value: match[3].trim() });
    }
  }
  return markers;
}

/**
 * The newest message of the user in the newest `window` marker of a room,
 * or undefined.
 */
export function lastUserMessage(markers, room) {
  const window = markers
    .filter((marker) => marker.key === "window" && marker.room === room)
    .at(-1);
  if (!window) return undefined;
  const ids = [...window.value.matchAll(/([^,=\s]+):out-[a-z]+:/g)].map(
    (match) => match[1],
  );
  return ids.at(-1);
}

/**
 * The newest message of another participant in the newest `window` marker
 * of a room, or undefined.
 */
export function lastProductMessage(markers, room) {
  const window = markers
    .filter((marker) => marker.key === "window" && marker.room === room)
    .at(-1);
  if (!window) return undefined;
  const ids = [...window.value.matchAll(/([^,=\s]+):in-[a-z]+:/g)].map(
    (match) => match[1],
  );
  return ids.at(-1);
}

/** True when an event log entry is the one an expectation names. */
export function eventMatches(entry, expected, room) {
  if (expected.via && entry.via !== expected.via) return false;
  if (expected.kind && entry.kind !== expected.kind) return false;
  if (
    (expected.room ?? room) &&
    entry.room &&
    entry.room !== (expected.room ?? room)
  ) {
    return false;
  }
  if (
    expected.data &&
    !JSON.stringify(entry.data ?? {}).includes(expected.data)
  ) {
    return false;
  }
  if (expected.hostId && !(entry.data?.messageId ?? "")) return false;
  return true;
}

/**
 * The reason the app gave for a call it does not serve, as the product saw
 * it: the `Rejected` reason that names `request`, or undefined.
 */
export function refusalReason(text, request) {
  const pattern = new RegExp(`this host does not serve chat ${request}`);
  const line = text.split(/\r?\n/).find((candidate) => pattern.test(candidate));
  return line
    ? `Rejected (this host does not serve chat ${request})`
    : undefined;
}

/**
 * Why the app did not start, from its console output, or undefined. The app
 * needs Firebase Remote Config for its chains and services: a build with
 * the placeholder `GoogleService-Info.plist` stops at "Configuration is
 * broken" before any product runs.
 */
export function appStartFailure(text) {
  if (/projects\/your-firebase-project-id\//.test(text)) {
    return 'the app has the placeholder GoogleService-Info.plist, so Firebase Remote Config fails and the app stops at "Configuration is broken"';
  }
  const failed = text
    .split(/\r?\n/)
    .find((line) => /FirebaseFacade\.handleFetchFailure/.test(line));
  return failed
    ? `Firebase Remote Config failed: ${failed.replace(/^.*handleFetchFailure\(\):\d+ - /, "").slice(0, 160)}`
    : undefined;
}

/**
 * The rows of the scenarios that the launcher could not run, because the
 * app did not start: the driven scenarios fail with `reason`, the others
 * keep their skip.
 */
export function blockedRows(reason, scenarios = IOS_CHAT_V2_SCENARIOS) {
  return scenarios.map((scenario) =>
    scenario.drive
      ? row(scenario, "fail", 0, `not run: ${reason}`)
      : skipRow(scenario),
  );
}

/** The `skip` row of a scenario that the launcher does not drive. */
export function skipRow(scenario, observed) {
  const reason = `skip: ${scenario.skip ?? `needs ${scenario.phase}`}`;
  return row(scenario, "skip", 0, observed ? `${reason}; ${observed}` : reason);
}

export function row(scenario, result, durationMs, evidence) {
  return {
    id: scenario.id,
    title: scenario.title,
    mode: scenario.mode,
    result,
    durationMs: Math.round(durationMs),
    evidence,
  };
}
