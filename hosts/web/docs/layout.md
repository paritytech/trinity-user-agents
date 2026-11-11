# Layout

Rules, from [`AGENTS.md`](../AGENTS.md): code lives with the feature that owns it. Tools, fixtures and tests sit outside
the features, and no feature imports them. Only `src/main.ts` and `src/app/` mount the tools. Features use only two
parts of `src/app/`: `prompt.ts` and `avatar-color.ts`.

Three owners, which [`tests/unit/boundaries.test.ts`](../tests/unit/boundaries.test.ts) checks on every relative import:

- `src/` is the host. It imports no product. Only `src/main.ts` and `src/app/chat-products.ts` name the products it
  bundles.
  - `src/integrations/<name>/` is the host side of one product's own protocol. It may use the host and the bridges, but
    no product, tool, fixture or other integration. The rest of the host imports neither it nor its bridge,
    `bridges/<name>/`; only the composition wires it in.
- `apps/` holds the products, one folder each. A product imports only its own folder, `apps/shared/` and `bridges/`.
- `bridges/` holds what both sides read. A bridge imports only other bridges.

## `src/`

- `main.ts`: the composition root. It builds the features, connects their callbacks and boots the core.
- `app/`: the shell.
  - Navigation and session: `navigation.ts` (front and held products, destinations, `navigateTo`), `session.ts` (the
    signed-in wallet and what a change reopens), `tab-session.ts` (who the tab acts as: the wallet, or the dev server's
    demo user, which `tools/demos/demo-session.ts` implements).
  - Dock and search: `shell.ts`, `dock-defaults.ts`, `apps-shelf.ts`, `browse-landing.ts`, `search-card.ts`,
    `address.ts` (address grammar), `address-recents.ts`, `recents.ts`.
  - Layout: `chrome.ts`, `layout.ts`, `device-mode.ts`, `keyboard-viewport.ts`, `sheet-handle.ts` (a sheet's grab
    handle: a tap or a drag down closes it).
  - Prompts and callbacks: `callbacks.ts`, `prompt.ts`, `reviews.ts`, `requester.ts` (where the requester came from).
  - Account and Product tabs: `wallet-label.ts`, `wallet-panel.ts`, `account-status.ts`, `permissions-status.ts`,
    `product-chain.ts`, `stack-menu.ts` (Stack), `chain-backends.ts` (Chain connections), `versions.ts`.
  - Helpers: `avatar-color.ts`, `copy.ts`, `serial-queue.ts`.
  - Bundled products: `chat-products.ts` (Contact Chat, in every build).
- `platform/`: browser and chain facilities.
  - Networks: `network.ts`, `network-config.ts`, `stacks.ts`, `endpoint-check.ts`, `network-scope.ts`,
    `network-choice.ts`, `follow-spread.ts`.
  - Storage and wallets: `storage.ts`, `wallets.ts`, `allowance-ledger.ts`, `dev-accounts.ts`.
  - Chain reads: `people-chain.ts`, `chain-directory.ts` (chat keys, usernames, owners), `subscription.ts`.
  - Statement Store: `statement-store.ts`, `statement-observer.ts`, `allowance-finality.ts` (grant reads, and the wait
    until a claim's allowance is final), `allowance-usable.ts` (each network's posting rule, and the wait until a
    claimed account may post), `guarded-post.ts` (a statement the host posts after a claim).
  - Other: `core-worker.ts` (the core's Web Worker), `activity.ts`.
- `chat/`: `provider.ts`, `inbox.ts`, `screen.ts`, `tabs.ts`, `conversation-view.ts`, `call-view.ts`.
  - TrUAPI Chat: `product-chat.ts`, `product-room-provider.ts`.
  - `draft/`: the host's end of the v2 bridge: `host.ts`, `provider.ts`, `calls.ts`. Its contract and the product's end
    are in `bridges/chat-v2/`.
- `contacts/`: `contacts.ts` (picker, handles), `wallet-contacts.ts`, `host-names.ts`, `username-directory.ts`,
  `username-search.ts`, `search-people.ts`.
- `products/`: `product.ts`, `tap.ts`, `worker-manifest.ts`, `product-workers.ts`, `workers-menu.ts`,
  `worker-console.ts`, `load-observer.ts`, `dotns.ts`, `preimage.ts`, `seed.ts`, `loading.ts`, `status.ts`, `render.ts`.
  - `archive/`: CAR, UnixFS and CID checks and unpacking.
  - `loader/`: the loader page, service worker, mounts, cache and console capture.
- `pocket/`: `pocket.ts`, `screen.ts`, `faces.ts`, `card-view.ts`, `widget.ts`.
- `notifications/`: the Inbox store and screen.
- `invitations/`: the request service. Files are listed in [Request service](request-service.md#code). A chat product
  with a setup of its own hands the request service a `start` function (`ChatProductSpec.start` in `wallet-chat.ts`).
- `integrations/nova-chat/`: Nova Chat's host side (prototype), the only part that holds the user's keys for it:
  `boundary.ts` (the key boundary), `crypto.ts`, `topics.ts`, `host.ts` (requests, acceptances and the worker's port),
  `start.ts` (`startNovaChat`, which `tools/chat/nova.ts` puts in Nova Chat's spec, and its prompts' costs).
- `backup/`: the wallet's backup on the network: `host-backup.ts` (keeping and restoring), `snapshot.ts` (what is
  copied), `sealing.ts` (keys and envelopes), `pointer.ts`. See [Backup](backup.md).
- `assets/`: fonts and Pocket artwork, with licence notices.
- `fields/`: the host side of the field demo, dev server only: `host.ts`, `transport.ts`.

## `bridges/`

What the host and its products both read. The contracts are drafts, not TrUAPI.

- `chat-v2/`: the Chat Extension v2 draft bridge: `contract.ts`, and the product's end, `product.ts` and
  `conversations.ts`.
- `host-requests/`: the request service's contract, `contract.ts`, and the product's end, `product.ts`.
- `nova-chat/`: `nova-chat/1` between the host and Nova Chat's worker, `contract.ts`, and the Nova apps' wire,
  `wire.ts`.
- `fields/protocol.ts`: `web-field-demo/1`.
- `diagnostics.ts`: what a product worker reports about its conversations to the developer tools.
- `worker-probe.ts`: a chat worker's status on its console, and the test probe a dev server tab may ask for.
- `primitives/`: `bytes.ts` (hex, UTF-8, randomness), `codec.ts` and `scale-reader.ts` (SCALE), `crypto.ts` (hashes,
  X25519, sr25519 checks), `statement.ts` (Statement Store statements).

## `apps/`

The products the host runs, each loaded through the ordinary loader.

- `contact-chat/`: Contact Chat, in every build: `worker.ts`, `engine.ts`, `protocol.ts`, `simple-text.ts`,
  `transport.ts`, `route.ts` (where the host serves its worker).
- `kettle-chat/`: the Kettle Chat prototype, dev server or `WEB_SIGNING_KETTLE_CHAT=1`: `worker.ts`, `engine.ts`,
  `ports.ts`, `route.ts`. `wasm/` is a Rust crate, built on its own with `npm run build:kettle`: Kettle's engine as
  WebAssembly. Its output, `wasm/pkg/`, is not committed. See
  [Kettle Chat](contact-chat.md#kettle-chat-dev-server-prototype).
- `nova-chat/`: the Nova Chat prototype, wherever Kettle Chat is: `worker.ts`, `session.ts`, `transport.ts`, `route.ts`.
  See [Nova Chat](nova-chat.md).
- `field-demo/`: the provider app (`provider.html`, run as Field notebook and as Travel notebook) and Small Market
  (`market.html`), dev only, served on their own origins by `field-products.config.ts`. Only the origins in `FIELD_APPS`
  (`tools/demos/field-demo.ts`) get the field bridge.
- `shared/product-storage.ts`: a chat product's storage over its TrUAPI `localStorage`.

## `tools/`

The developer tools, in every build.

- `mount.ts`, `viewport.ts`, `dock-resize.ts`: the inspector mount and the Preview tab.
- `inspector/frame-tap.ts`: TrUAPI frames to the debugger.
- `activity/`, `workers/`, `chat/`, `products/`: those inspector views. `chat/` also holds the Chat tab (`products.ts`)
  and the specs of Kettle Chat (`kettle.ts`, with the record of where its accounts became usable, for its live test) and
  Nova Chat (`nova.ts`), which only the dev server and a build with `WEB_SIGNING_KETTLE_CHAT=1` load.
- `statements/`: `ledger.ts`, `decode.ts`, `topics.ts`, `actions.ts` (grant read), `evidence.ts`, `transport.ts`
  (wording per People backend), `ui.ts`, `view.ts`.
- `demos/`: dev server only, left out of a build: `dev-tools.ts`, `local-demo-route.ts` (loopback check),
  `no-submit.ts`, `kettle-demo.ts` (the demo's simulated Statement Store and allowance), `backup-demo.ts` (the simulated
  network a browser test runs for the backup), `wallet-deliveries.ts` (the Developer section's other request
  deliveries), `demo-session.ts` (the tab as a demo user), `worker-probes.ts` (which chat workers start with their test
  probe), `field-demo.ts`, `field-view.*` and `field-label.ts` (the field demo's wiring, host-drawn view and label).

## Other

- `fixtures/`: synthetic data for tests and the dev server: `products/`, `invitations/` (demo users, local delivery,
  shared store), `statements/` (the simulated Statement Store), `notifications/`, `archive/`.
- `tests/unit/` follows the code that it covers. `tests/support/` holds helpers, including in-memory statement stores.
- `browser/`: browser tests. `browser/support/` holds the Playwright setup.
- Vite plugins: `sandbox-plugin.ts` (loader and service worker), `contact-chat-plugin.ts`, `browse-seed-plugin.ts`,
  `username-search-plugin.ts`.
