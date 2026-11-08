# Layout

Rules, from [`AGENTS.md`](../AGENTS.md): code lives with the feature that owns it. Tools, fixtures and tests sit outside
the features, and no feature imports them. Only `src/main.ts` and `src/app/` mount the tools. Features use only two
parts of `src/app/`: `prompt.ts` and `avatar-color.ts`.

## `src/`

- `main.ts`: the composition root. It builds the features, connects their callbacks and boots the core.
- `app/`: the shell.
  - Navigation and session: `navigation.ts` (front and held products, destinations, `navigateTo`), `session.ts` (the
    signed-in wallet and what a change reopens).
  - Dock and search: `shell.ts`, `dock-defaults.ts`, `apps-shelf.ts`, `browse-landing.ts`, `search-card.ts`,
    `address.ts` (address grammar), `address-recents.ts`, `recents.ts`.
  - Layout: `chrome.ts`, `layout.ts`, `device-mode.ts`, `keyboard-viewport.ts`, `sheet-handle.ts` (a sheet's grab
    handle: a tap or a drag down closes it).
  - Prompts and callbacks: `callbacks.ts`, `prompt.ts`, `reviews.ts`, `requester.ts` (where the requester came from).
  - Account and Product tabs: `wallet-label.ts`, `wallet-panel.ts`, `account-status.ts`, `permissions-status.ts`,
    `product-chain.ts`, `chain-backends.ts` (Chain connections), `versions.ts`.
  - Helpers: `avatar-color.ts`, `copy.ts`, `serial-queue.ts`.
- `platform/`: browser and chain facilities.
  - Networks: `network.ts`, `network-config.ts`, `network-scope.ts`, `network-choice.ts`, `follow-spread.ts`.
  - Storage and wallets: `storage.ts`, `wallets.ts`, `allowance-ledger.ts`, `dev-accounts.ts`.
  - Chain reads: `people-chain.ts`, `chain-directory.ts` (chat keys, usernames, owners), `scale-reader.ts`,
    `subscription.ts`.
  - Statement Store: `statement.ts`, `statement-store.ts`, `statement-observer.ts`.
  - Other: `core-worker.ts` (the core's Web Worker), `activity.ts`, `codec.ts`, `crypto.ts`.
- `chat/`: `provider.ts`, `inbox.ts`, `screen.ts`, `tabs.ts`, `conversation-view.ts`, `call-view.ts`.
  - TrUAPI Chat: `product-chat.ts`, `product-room-provider.ts`, `worker/product-storage.ts`.
  - `draft/`: the v2 bridge: `contract.ts`, `host.ts`, `provider.ts`, `calls.ts`, `product.ts`, `conversations.ts`.
  - `contact-chat/`: `worker.ts`, `engine.ts`, `protocol.ts`, `simple-text.ts`, `transport.ts`, `requests.ts`,
    `route.ts`.
  - `kettle-chat/`: the Kettle Chat prototype, dev server only: `worker.ts`, `engine.ts`, `ports.ts`, `route.ts`. See
    [Kettle Chat](contact-chat.md#kettle-chat-dev-server-prototype).
- `contacts/`: `contacts.ts` (picker, handles), `wallet-contacts.ts`, `host-names.ts`, `username-directory.ts`,
  `username-search.ts`, `search-people.ts`.
- `products/`: `product.ts`, `tap.ts`, `worker-manifest.ts`, `product-workers.ts`, `workers-menu.ts`,
  `worker-console.ts`, `diagnostics.ts`, `load-observer.ts`, `dotns.ts`, `preimage.ts`, `seed.ts`, `loading.ts`,
  `status.ts`, `render.ts`.
  - `archive/`: CAR, UnixFS and CID checks and unpacking.
  - `loader/`: the loader page, service worker, mounts, cache and console capture.
- `pocket/`: `pocket.ts`, `screen.ts`, `faces.ts`, `card-view.ts`, `widget.ts`.
- `notifications/`: the Inbox store and screen.
- `invitations/`: the request service. Files are listed in [Request service](request-service.md#code).
- `assets/`: fonts and Pocket artwork, with licence notices.

## `tools/`

The developer tools, in every build.

- `mount.ts`, `viewport.ts`, `dock-resize.ts`: the inspector mount and the Preview tab.
- `inspector/frame-tap.ts`: TrUAPI frames to the debugger.
- `activity/`, `workers/`, `chat/`, `products/`: those inspector views. `chat/` also holds the Chat tab (`products.ts`)
  and Kettle Chat's product and finality guard (`kettle.ts`), which only the dev server and a build with
  `WEB_SIGNING_KETTLE_CHAT=1` load.
- `statements/`: `ledger.ts`, `decode.ts`, `topics.ts`, `actions.ts` (grant read), `evidence.ts`, `transport.ts`
  (wording per People backend), `ui.ts`, `view.ts`.
- `demos/`: dev server only, left out of a build: `dev-tools.ts`, `local-demo-route.ts` (loopback check),
  `no-submit.ts`, `kettle-demo.ts` (the demo's simulated Statement Store and allowance).

## Other

- `fixtures/`: synthetic data for tests and the dev server: `products/`, `invitations/` (demo users, local delivery,
  shared store), `statements/` (the simulated Statement Store), `notifications/`, `archive/`.
- `kettle-chat-wasm/`: a Rust crate, built on its own with `npm run build:kettle`: Kettle's engine as WebAssembly for
  Kettle Chat. Its output, `pkg/`, is not committed.
- `tests/unit/` follows the code that it covers. `tests/support/` holds helpers, including in-memory statement stores.
- `browser/`: browser tests. `browser/support/` holds the Playwright setup.
- Vite plugins: `sandbox-plugin.ts` (loader and service worker), `contact-chat-plugin.ts`, `browse-seed-plugin.ts`,
  `username-search-plugin.ts`.
