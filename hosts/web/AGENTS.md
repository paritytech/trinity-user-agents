# TrUAPI Web Host

A browser host for trying products against the current core: it holds its own development wallets, embeds products, runs
the core in a Web Worker and signs locally. It is a development tool and a reference for how a host and a product meet,
not a production host and not a sandbox; read [Trust model](README.md#trust-model) before changing what it trusts. The
app side should behave like the Polkadot app; developer controls stay in the developer tools, outside the app.

The [README](README.md) owns the details: [Layout](README.md#layout) maps every folder and file, and
[Tests](README.md#tests) lists the commands and what each browser test checks.

## Where code goes

Put code with the feature that owns its behavior, not with the screen or tool that happens to use it first.

- `src/main.ts` is the composition root: it builds the features, wires their callbacks and boots the core. Keep logic
  that has an owner out of it.
- `src/app/` is the shell around the features: navigation (the product in front and the ones held behind), the wallet
  session and the order a session reaches what belongs to it, the dock, the address bar, prompts and host callbacks.
- `src/platform/` holds browser and chain facilities: network, storage, wallets, the core's Web Worker, and the event
  contract the Activity view reads.
- Feature folders own their behavior: `src/chat/`, `src/contacts/`, `src/products/` (frames, product workers,
  `archive/`, `loader/`), `src/pocket/`, `src/notifications/`, `src/invitations/`.
- `tools/` is the developer tools: the inspector views and the Preview tab ship in every build. `tools/demos/` holds
  dev-server controls that a build leaves out.
- `fixtures/` is synthetic data and products for tests and the dev server only. `tests/unit/` follows the code it
  covers, `tests/support/` holds shared helpers, and `browser/` holds the browser tests with `browser/support/`.

## Boundaries to keep

- Features do not import `tools/`, `fixtures/` or tests. Only the composition (`src/main.ts`, `src/app/`) mounts tools.
  When a tool needs to see a feature, the feature exposes a small observer or event contract next to itself, and the
  tool implements it.
- Dev-server-only code stays out of a build: demos and fixtures load behind the existing dev-server gate in
  `src/main.ts`. Check that a build still leaves them out when you add one.
- Chat has one Nova engine, `src/chat/nova/`, used by both the host page and the Nova chat worker. Add adapters around
  it (`nova-session.ts` for the page, `src/chat/worker/` for the worker); never a second engine or a copy of its
  protocol. The Nova chat worker is a real product under `src/chat/`, not a tool, even though only the dev server builds
  it.
- Keep official TrUAPI and drafts apart. Official product rooms are `src/chat/product-chat.ts` and
  `product-room-provider.ts`; the Chat Extension v2 bridge and the Nova identity bridge live in `src/chat/draft/` and
  are labelled draft. Use canonical types from `@parity/truapi` and `@parity/truapi-host`; do not copy or redefine them.
- A demo product runs as a product: loaded through the ordinary loader and reaching the host only through TrUAPI or its
  declared bridge. Do not reproduce a product's actions as host buttons, which would skip the path it tests.
- Developer views show state the host actually holds, never state inferred from logs. Keep what they record bounded and
  in memory, keep payload capture opt-in, and never record private keys, shared secrets, recovery phrases or
  authentication credentials. Public keys, topics and protocol bytes are what the inspector is for.

## Adding things

- Prefer an existing owner and its contracts before adding a module or an abstraction. Add no framework, registry or
  plugin layer to make room for a fixture or a demo.
- Use plain technical names. Comment only a reason the code cannot show.
- Prototype designs (invitations, the draft bridges, delivery, fixture identities) are prototypes. Follow their current
  contracts, but do not treat their routes, keys or protocols as settled.

## Keeping this file current

When a change moves an owner, adds a top-level folder or changes a boundary above, update this file and README
[Layout](README.md#layout) in the same change. Keep the file tree, status and specifications in the README and the
design docs, not here.
