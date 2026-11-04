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

## Browser tests

Prerequisites: the linked packages and both WASM bundles (see [Run the dev server](running.md#run-the-dev-server)), and
a Chromium (`npx playwright-core install chromium`, or set `CHROMIUM_PATH`).

- The specs start the host with Vite on loopback. Most close every socket and refuse every request outside this host.
  They use synthetic or public test phrases. No browser test claims or posts on chain.
- Each spec file opens with a comment that states its setup. Its test names state what it checks.
- `CHAT_SCREENSHOTS` or `STATEMENTS_SCREENSHOT` set to a directory keeps screenshots.

- [`smoke.spec.ts`](../browser/smoke.spec.ts): names and URLs, loading, the dock, workers, contacts, chat products,
  Humanity.
- [`build.spec.ts`](../browser/build.spec.ts): the static build, the Browse seed (`WEB_SIGNING_SEED_BROWSE=0`: none),
  Contact Chat's archive.
- [`mobile.spec.ts`](../browser/mobile.spec.ts): an emulated phone browser: gestures, zoom lock, full screen.
- [`prompts.spec.ts`](../browser/prompts.spec.ts): the host's prompts.
- [`app-area.spec.ts`](../browser/app-area.spec.ts): where a product in front sits.
- [`docklet-search.spec.ts`](../browser/docklet-search.spec.ts): the search field.
- [`keyboard.spec.ts`](../browser/keyboard.spec.ts): an on-screen keyboard (a simulated visual viewport).
- [`open-apps.spec.ts`](../browser/open-apps.spec.ts): Browse, the Apps shelf, `navigateTo`.
- [`preimage-image.spec.ts`](../browser/preimage-image.spec.ts): preimage lookups.
- [`workers-menu.spec.ts`](../browser/workers-menu.spec.ts): Product → Workers.
- [`chat-screen.spec.ts`](../browser/chat-screen.spec.ts): the Chat screen over its providers, New chat, blocking.
- [`notifications.spec.ts`](../browser/notifications.spec.ts): the Inbox.
- [`contacts-pick.spec.ts`](../browser/contacts-pick.spec.ts): `contacts.pick` for demo users.
- [`invitations.spec.ts`](../browser/invitations.spec.ts): demo users, test accounts, invitations.
- [`contact-chat.spec.ts`](../browser/contact-chat.spec.ts): Contact Chat between two demo users.
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

## Not covered

- The smoke test runs on the Vite dev server. It does not prove that a static host serves the build. Open the built
  `dist/` from the host that you will use.
- Tried by hand in Chrome when the behaviour changes: real camera and microphone, a call or file transfer with a
  Polkadot app, the phone setup, two tabs with two wallets, a build under a project path, the inspector's rendering, and
  narrow layouts.
