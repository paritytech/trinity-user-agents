# Tests

Run from `hosts/web`:

```bash
npm run typecheck      # tsc --noEmit
npm test               # unit tests under tests/unit/ (bun test, fake gateway, no network)
npm run build          # tsc --noEmit && vite build
npm run test:browser   # every spec under browser/, in headless Chrome
```

- CI runs only `typecheck` and `npm test` (the `host-web` job in `.github/workflows/ci.yml`). The build and browser
  tests need the signing core's WASM bundle. Run them by hand when the host or the core changes.
- These gates catch a broken host, not a hostile product.
- [`tests/unit/chat/kettle-chat/wasm.test.ts`](../tests/unit/chat/kettle-chat/wasm.test.ts) runs Kettle's WebAssembly
  over scripted ports, and [`tests/unit/backup/kettle-restore.test.ts`](../tests/unit/backup/kettle-restore.test.ts)
  restores its real MLS state from a backup. Both are skipped until `npm run build:kettle` has run, so CI skips them.

## Browser tests

Prerequisites: the linked packages and both WASM bundles (see [Run the dev server](running.md#run-the-dev-server)), and
a Chromium (`npx playwright-core install chromium`, or set `CHROMIUM_PATH`).

- The specs start the host with Vite on loopback. Most close every socket and refuse every request outside this host.
  They use synthetic or public test phrases. No browser test claims or posts on chain.
- Each spec file opens with a comment that states its setup. Its test names state what it checks.
- Specs that read a chat worker's state start the workers with their test probe (`probeChatWorkers` in
  `browser/support/fixtures.ts`). Build specs have none, and read the Workers view instead.
- `CHAT_SCREENSHOTS` or `STATEMENTS_SCREENSHOT` set to a directory keeps screenshots.

- [`smoke.spec.ts`](../browser/smoke.spec.ts): names and URLs, loading, the dock, workers, contacts, chat products,
  Humanity.
- [`build.spec.ts`](../browser/build.spec.ts): the static build, the Browse seed (`WEB_SIGNING_SEED_BROWSE=0`: none),
  Contact Chat's archive.
- [`mobile.spec.ts`](../browser/mobile.spec.ts): an emulated phone browser: gestures, zoom lock, full screen, and the
  Developer sheet's and search card's handles.
- [`prompts.spec.ts`](../browser/prompts.spec.ts): the host's prompts, and how each is dismissed.
- [`app-area.spec.ts`](../browser/app-area.spec.ts): where a product in front sits.
- [`docklet-search.spec.ts`](../browser/docklet-search.spec.ts): the search field.
- [`keyboard.spec.ts`](../browser/keyboard.spec.ts): an on-screen keyboard (a simulated visual viewport).
- [`open-apps.spec.ts`](../browser/open-apps.spec.ts): Browse, the Apps shelf, `navigateTo`.
- [`preimage-image.spec.ts`](../browser/preimage-image.spec.ts): preimage lookups.
- [`workers-menu.spec.ts`](../browser/workers-menu.spec.ts): Product → Workers.
- [`backup.spec.ts`](../browser/backup.spec.ts): the backup between two browser profiles of one wallet, on a simulated
  network the test runs (`tools/demos/backup-demo.ts`), with the backup on by default and the core deriving the backup
  account and signing the pointer: a fresh profile restores with no prompt and nothing claimed, stored or posted; one
  that holds the wallet keeps its state; one that cannot read the backup keeps none of its own; one tab holds a wallet's
  writer lock, and a wallet switch hands it on.
- [`chat-screen.spec.ts`](../browser/chat-screen.spec.ts): the Chat screen over its providers, New chat, blocking.
- [`notifications.spec.ts`](../browser/notifications.spec.ts): the Inbox.
- [`contacts-pick.spec.ts`](../browser/contacts-pick.spec.ts): `contacts.pick` for demo users.
- [`invitations.spec.ts`](../browser/invitations.spec.ts): demo users, test accounts, invitations.
- [`contact-chat.spec.ts`](../browser/contact-chat.spec.ts): Contact Chat between two demo users.
- [`chat-products.spec.ts`](../browser/chat-products.spec.ts): the developer tools' Chat tab: which chat products run,
  New chat's product, and both held across reloads.
- [`kettle-build.spec.ts`](../browser/kettle-build.spec.ts): a build with `WEB_SIGNING_KETTLE_CHAT=1`: Kettle Chat's
  archive, no dev demos, and a wallet running Kettle Chat alone from it. Skipped until `npm run build:kettle` has run.
- [`nova-chat.spec.ts`](../browser/nova-chat.spec.ts): Nova Chat between three demo users, over the simulated Statement
  Store, both directions and a reload, and its conversation in the inspector's Chat view with no test probe.
- [`kettle-chat.spec.ts`](../browser/kettle-chat.spec.ts): Kettle Chat between two demo users, over the simulated
  Statement Store, alone and beside Contact Chat. Skipped until `npm run build:kettle` has run.
- [`kettle-iroh.spec.ts`](../browser/kettle-iroh.spec.ts): Kettle Chat's log recovery over iroh, between two demo users,
  through two `iroh-relay`s this test runs on loopback. A relay not reached falls back to the Statement Store; a side
  with iroh off asks for no socket; iroh turned on and off mid-chat keeps the history. Bob away for more than one outbox
  gets every message by a log fetch while Alice's new outboxes are held out of the store; the control without iroh gets
  only the stored window until they land; a peer on another relay gets no connection. Every socket a tab asks for is
  recorded, and any but the given relays fails the test. The public test relays are the default, kept per tab; with them
  unreachable (blocked here) the chat runs on the Statement Store alone. `KETTLE_IROH_PUBLIC=1` adds one recovery smoke
  over n0's public relays, run by hand only. Needs `npm run build:kettle` and `iroh-relay` 1.4.1 (`IROH_RELAY_BIN` or
  PATH); skipped otherwise.
- [`rpc-socket-close.spec.ts`](../browser/rpc-socket-close.spec.ts): the provider's RPC socket, closed and released or
  failing its handshake, throws nothing into the page.
- [`inspector-dock.spec.ts`](../browser/inspector-dock.spec.ts): the tools' dock, resize and Focus.
- [`workers-inspector.spec.ts`](../browser/workers-inspector.spec.ts): the Workers view and console.
- [`products-inspector.spec.ts`](../browser/products-inspector.spec.ts): the Products view.
- [`activity-log.spec.ts`](../browser/activity-log.spec.ts): the Activity view.
- [`statements-inspector.spec.ts`](../browser/statements-inspector.spec.ts): the Statements and Chat views.

### Live specs

[`wallet-delivery-live.spec.ts`](../browser/wallet-delivery-live.spec.ts) and
[`chat-claims-live.spec.ts`](../browser/chat-claims-live.spec.ts) run only with `TRUAPI_LIVE=1`, against Paseo Next v2.

- They sign in the public test accounts and start a chat with `testbob.10`.
- They use the "validate claims only" modes. They check the claim that the live runtime would accept, and that the
  no-submit guard passed no submit or broadcast.
- A failed run keeps each tab's guard record, Activity lines and a screenshot under `TRUAPI_LIVE_EVIDENCE`, or the
  temporary directory.

[`kettle-chat-live.spec.ts`](../browser/kettle-chat-live.spec.ts) runs only with `TRUAPI_LIVE=1 TRUAPI_LIVE_SUBMIT=1`,
and after `npm run build:kettle`. It **submits**: Kettle Chat between `testalice.10` and `testbob.10` on Paseo Next v2,
two notification slots and two general slots per run. It reads the conversation's statements back from People's RPC node
and keeps a report, passed or failed, under `TRUAPI_LIVE_EVIDENCE`.

## Not covered

- The smoke test runs on the Vite dev server. It does not prove that a static host serves the build. Open the built
  `dist/` from the host that you will use.
- Tried by hand in Chrome when the behaviour changes: real camera and microphone, a call or file transfer with a
  Polkadot app, the phone setup, two tabs with two wallets, a build under a project path, the inspector's rendering, and
  narrow layouts.
