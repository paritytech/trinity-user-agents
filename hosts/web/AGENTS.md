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
  `archive/`, `loader/`), `src/pocket/`, `src/notifications/`, `src/invitations/`, `src/backup/`. The host side of each
  bridge lives with its feature.
- `src/app/chat-products.ts` names the chat product every build bundles (Contact Chat). With `src/main.ts`, it is the
  only host code that names a product.
- `src/integrations/<name>/` is the host side of one product's own protocol, such as Nova Chat's key boundary and
  request host. It uses host services and the bridges. General host features do not import it or its bridge
  (`bridges/<name>/`); the composition wires it in through a seam the feature exposes.
- `apps/` holds the products the host runs, one folder each: `contact-chat/`, `kettle-chat/` (its Rust engine in
  `wasm/`), `nova-chat/`, `field-demo/`. `apps/shared/` is code only products share.
- `bridges/` holds what the host and its products both read: the draft contracts between them and the pure byte, SCALE,
  hash and statement code in `primitives/`. It holds no logic that belongs to one side.
- `tools/` is the developer tools: the inspector views and the Preview tab ship in every build. `tools/demos/` holds
  dev-server controls that a build leaves out.
- `fixtures/` is synthetic data and synthetic test products for tests and the dev server only. A product a person can
  use, even one only the dev server runs, goes in `apps/`. `tests/unit/` follows the code it covers, `tests/support/`
  holds shared helpers, and `browser/` holds the browser tests with `browser/support/`.

## Boundaries to keep

- `tests/unit/boundaries.test.ts` checks every relative import: the host imports no product (except `src/main.ts` and
  `src/app/chat-products.ts`), a product imports only its own folder, `apps/shared/` and `bridges/`, a bridge imports
  only bridges, and an integration stays apart from the rest of the host as described above. Fix a new violation; do not
  add it to the test's legacy list, which only shrinks.
- Features do not import `tools/`, `fixtures/` or tests. Only the composition (`src/main.ts`, `src/app/`) mounts tools.
  When a tool needs to see a feature, the feature exposes a small observer or event contract next to itself, and the
  tool implements it.
- Dev-server-only code stays out of a build: demos and fixtures load behind the existing dev-server gate in
  `src/main.ts`. Check that a build still leaves them out when you add one.
- Nova Chat is split by who holds what. The host holds the user's keys and does every step that needs one, in its
  integration `src/integrations/nova-chat/` (a prototype). The worker, `apps/nova-chat/`, keeps the conversations and
  has no key. `bridges/nova-chat/` holds only the port contract between them (`nova-chat/1`) and the Nova apps' wire.
  Keep one copy of each; never a second implementation of the protocol on either side. The worker is a real product, not
  a tool, even though only the dev server and a build with `WEB_SIGNING_KETTLE_CHAT=1` serve it.
- Keep official TrUAPI and drafts apart. Official product rooms are `src/chat/product-chat.ts` and
  `product-room-provider.ts`. The draft bridges are labelled draft: their contracts and product ends are in `bridges/`
  (`chat-v2/`, `host-requests/`, `nova-chat/`, `fields/`), and the host's end of Chat Extension v2 is `src/chat/draft/`.
  Use canonical types from `@parity/truapi` and `@parity/truapi-host`; do not copy or redefine them.
- A demo product runs as a product: loaded through the ordinary loader and reaching the host only through TrUAPI or its
  declared bridge. Do not reproduce a product's actions as host buttons, which would skip the path it tests.
- The field demo's bridge goes only to the exact origins and pages in `FIELD_APPS` (`tools/demos/field-demo.ts`), each
  its own provider or market. Never give a role from what a page declares about itself.
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
