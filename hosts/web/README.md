# TrUAPI Web Host

A TrUAPI host in one browser tab. It holds its own wallet, embeds a product in an iframe, runs the core from this
repository in a Web Worker, and signs locally. There is no companion app and no QR pairing.

It is a development tool, not a production host. The production web host is dotli (`hosts/dotli/`), which pairs with the
Polkadot app. This host loads the `testing` core (built with `wasm-signing-host`) and keeps its recovery phrases in the
browser.

## Trust model

The host trusts its user and the products that the user opens. It is not a sandbox.

- Wallets are disposable. Recovery phrases are unencrypted in `localStorage`. Never import a phrase that holds value.
- A product opened by name runs on the host's origin and can read the saved wallets through `window.parent`. Open only
  products that you trust. A product opened by URL stays on its own origin.
- Separation by wallet is by storage name, not browser isolation. Prompts are a development aid, not custody.
- A product's own `fetch`, XHR, WebSocket, WebRTC and media follow the browser, not the core.
- Anyone who can reach a served host can use it. Serve it on loopback, or treat it as public.

Details: [docs/trust-model.md](docs/trust-model.md).

## Run it

From the repository root:

```bash
make codegen                                   # also builds @parity/truapi
npm run build --prefix js/packages/truapi-host
npm run build --prefix js/packages/truapi-debugger
npm run build --prefix js/packages/trinity-stack
make wasm                                      # both truapi-host bundles and truapi-provider
cd hosts/web && npm ci && npm run dev
```

1. Open <http://localhost:5180> and sign in.
2. Open the search card with the Docklet's middle button.
3. Type a product name or URL and press Enter.

The dev server answers `localhost` only. Set `WEB_SIGNING_ALLOWED_HOSTS=name.example` (a comma list) for other names.

## Build and deploy

```bash
npm run build -- --base /<repo>/   # GitHub Pages project site
npm run build                      # served from the root
```

- `dist/` is a static folder. The base must match the served path.
- A root build ships the Browse seed and reads Paseo Next v2 to make it. `WEB_SIGNING_SEED_BROWSE=0` builds offline.
- `WEB_SIGNING_KETTLE_CHAT=1` also builds in Kettle Chat and the developer tools' Chat tab, for a private test
  deployment, after `npm run build:kettle`. See [Kettle Chat](docs/contact-chat.md#kettle-chat-dev-server-prototype).
- A name opens only in a secure context: `https` or `http://localhost`.

Details, the username search route and phones: [docs/running.md](docs/running.md).

## Locket bridge demo (dev server only)

Locket is a web-only DRAFT of a generic bridge for the user's private details (`web-locket/1`), not TrUAPI. The contract
in `bridges/locket/` names no DOM, URL or browser store, so iOS and Android could implement the same calls later; only
`web.ts` and `protected-detail.ts` are the web form of drawing a detail. Nothing here changes TrUAPI wire ids, traits or
the native apps, and there is no `truapi.locket` namespace. The strings that derive or name stored keys, envelopes and
key bindings keep their `fields` names (such as `web-fields/envelope/v1`), because they are part of those bytes.

| Role                        | Calls                                                            | Types                                                                                                        |
| --------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| consumer (`locket`)         | `recipientKey`, `request`, `open`, `release`, `display`, `print` | `DetailKind`, `DetailRef`, `RecipientKey`, `DetailDescriptor`, `DetailRequest`, `DetailGrant`, `LocketError` |
| provider (`locketProvider`) | `status`, `declare`, `setValues`                                 | `DetailValues`, `ProviderStatus`                                                                             |

- A **provider** app (Field notebook, and the same app as Travel notebook on a second origin) declares `address` and
  `phone` and sets their values. The host keeps them in memory. Declaring the same name and kinds again is harmless; a
  different declaration fails `invalid`. `setValues` replaces every value and names each declared kind, `null` for none.
- A **consumer** app (Small Market) asks for typed `details`: here a required address and an optional phone. The host
  shows the exact values per provider in one sheet, and answers with descriptors `{ id, kind, schema, ref }` and, when
  the request names a recipient, a sealed envelope. The app never gets a value.
- The recipient's market asks its host for a **recipient key** (`recipientKey`) for one of its accounts, by account
  index. The host makes one random X25519 key per wallet, network, product and product account, keeps it encrypted, and
  returns its public key, key id, the product account the core resolved the index to, and a binding signature. The core
  signs that binding as the market's product account under its own sr25519 signing context, which no product signing
  call uses (`rust/crates/truapi/src/host_logic/field_key_binding.rs`, a prototype core extension). The buyer's host
  checks the signature through the core before it asks anything; the buyer does not compare keys by hand.
- **Delivery is the product's.** The host's calls and the envelope name no transport, order or topic: a product carries
  recipient keys and envelopes as opaque bytes over whatever it uses, such as the Statement Store now or a contract
  later. Its size limit, lifetime, delivery and replay handling are its own. Small Market posts listings and orders to
  its own board through its TrUAPI `statementStore` (`createProofAuthorized`, `submit`, `subscribe`) after
  `resourceAllocation` for a Statement Store allowance, and keeps its listings, sales and purchases in its TrUAPI
  storage (`apps/field-demo/board.ts`). The host does not post, list or buy.
- The seller's market gives the stored envelope and its order context back to `open`, and gets fresh refs. It places
  `mountProtectedDetail` (`bridges/locket/protected-detail.ts`) where the address and phone go; the host draws each as
  an SVG image (`src/locket/svg.ts`) that the dev server serves (`field-image-plugin.ts`). `print` prints the market's
  own title and lines with the held values from a host document.

From `hosts/web`, run these in separate terminals, with the normal host prerequisites above already built. The binding
calls need a core built from this tree (`make wasm`):

```bash
npm run dev                       # host, localhost:5180
npm run dev:field-provider         # Field notebook, separate origin, localhost:5182
npm run dev:field-provider-2       # Travel notebook, separate origin, localhost:5185
npm run dev:field-market           # separate origin, localhost:5183
```

Only these exact origins and pages may use the Locket bridge (`LOCKET_APPS` in `tools/demos/field-demo.ts`); any other
URL gets no bridge. Use two host tabs, each signed in through **Account**, and open each URL from **Product**:

1. Alice opens `http://localhost:5182/provider.html` and saves a made-up address and, optionally, a phone. Optionally
   she does the same in `http://localhost:5185/provider.html`.
2. Bob opens `http://localhost:5183/market.html`, goes to **Sell** and lists an item. The first post asks his host for
   the posting allowance and the submit permission (the core's own sheets).
3. Alice opens the market in the same tab, opens the item and selects **Buy now**. One host sheet shows the app's
   purpose, the recipient account and, per provider, the exact values: address with phone, or address only. Her first
   post then asks for the allowance and permission. Her receipt shows her details, drawn by her host.
4. Bob's **Orders** shows the order. Opening it shows the packing slip with the address and phone drawn by his host.
   **Print shipping label** prints from the host with no further sheet. A new host tab for Bob, signed in to the same
   wallet, opens the order again.

**Host sheets:** a sheet asks only when something leaves the host, spends a limited resource or is a real choice. Saving
in a provider, making the recipient key, opening an envelope, drawing and printing ask nothing. Sending details asks
once, with the exact values. Posting is the core's allowance and permission sheets. A sheet's values are read when it
opens; that snapshot is what gets sealed.

**Envelope** (`src/locket/envelope.ts`): `version (1) || suite (1) || key id (16) || sealed`, where `sealed` is X25519,
HKDF-SHA256 and ChaCha20-Poly1305 from `src/invitations/sealing.ts`, and the key derivation binds the version, suite,
key id and recipient public key. Inside, padded to 256-byte buckets with at most 1,024 bytes: the product, the recipient
account, the context and the details with their schemas (the JSON member is `fields`). Opening refuses a changed byte,
another key, product, account or context, and a duplicate detail id or kind as `invalid`; an unknown version, suite,
detail kind or schema as `unsupported`; either refuses the whole envelope. An envelope whose key id matches no recipient
key on this device fails `missing-key`: nothing restores a lost key, so if the envelope was meant for this account, the
sender has to send again. The code says nothing about who sealed it or for whom. The context labels one use (here the
listing and the order); it does not prove a purchase or who sent it. The sender is not authenticated. Opening the same
bytes twice gives the same `envelopeId` and new refs; spotting a replay is the product's job. The suite is a prototype
choice and is not reviewed for envelopes kept long term, for example on a chain: anyone who keeps the bytes keeps them,
and the suite is not post-quantum.

**Refs** (`DetailRef`) are random, held per bridge connection, and end on `release`, on the next load or close of the
document, and on sign-out; a ref that ended fails `missing-ref`. The product chooses where a detail goes and how wide;
the host chooses what it shows.

**Calls:** a product's `request`s wait for each other, so the user sees one sheet at a time; every other call runs at
once, so `display` and `release` do not wait behind an open sheet. At most 32 calls per connection wait at once; more
fail `busy`. A call the client still has no answer for after 180 seconds is cancelled at the host (`{ cancel: id }`):
the host stops its pending work, its sheet closes, nothing it would have returned is kept, and it fails `timeout`. Work
already done stays done, such as refs a `release` had let go, so retry. A result the host sent before it saw the cancel
is still the answer. The user's own Cancel fails `cancelled`. The codes are listed on `LocketErrorCode` in
`bridges/locket/protocol.ts`.

**Drawing:** `display` gives the product a surface only the platform adapter places. On the web it is an image URL the
product embeds with `<img>`. The host draws the value as SVG with `src/locket/svg.ts`: text only, escaped, no script,
link, style or image, at one fixed size per kind (address 360 × 150, phone 360 × 26), wrapping long values and then
using smaller type. Widths are estimated per grapheme, wide scripts and emoji as a whole em, so an unusually wide font
can still run past the edge; a value that does not fit is refused when it is saved, never cut. The dev server
(`field-image-plugin.ts`) holds that SVG in memory and serves it once, within 60 seconds, to a loopback request with
`Sec-Fetch-Dest: image` and a `Referer` of the product's origin, `no-store` and with no CORS grant. The browser keeps a
cross-origin image's text and pixels from the product's scripts. `release` and sign-out ask the dev server to end images
not loaded yet; that is best effort, and a failed request or a closed tab leaves them until they expire. This dev server
is the web stand-in for a native host's own local image scheme; native drawing is not built.

Known limits of this prototype, accepted: the image URL is a bearer link. Product frames are `credentialless`, so the
request carries no host cookie; until it is used, anything on this machine that gets the URL can send those two headers
and receive the SVG, value included, and a page of the product's origin in another browser profile can display it. An
image already shown stays on screen after release; sign-out closes the product's document. The dev server's 500 tokens
are shared by every product, so one product can fill them for a minute. A screen reader gets only the image's alt text.

**Recipient keys** (`src/locket/keys.ts`): one per wallet, network, product and product account, reused for every order
to that account, so two accounts of one product publish different keys. `open` reports the account an envelope was
sealed for. The secret is encrypted with XChaCha20-Poly1305 under a key derived from the signed-in wallet's entropy and
kept in this origin's `localStorage`, bound to its account; sign-out forgets the derived key and keeps the records.
Earlier builds kept one key per product, shared by its accounts: such a key is still read, only to open envelopes
already sealed to it, and is never bound again. Its accounts were linkable by anyone who saw both bindings, and they
stay so; new keys do not undo that. A stored record that cannot be read is kept as it is: no new key is written over it,
and opening or fetching a key then fails `unavailable` rather than `missing-key`. This development host keeps its
wallets' recovery phrases in the same `localStorage` in plain text, so this is no stronger than the wallet store and is
not a production key store. Same device only: no rotation, backup, other device or recovery. Clearing site data loses
the key and every order sealed to it. Use synthetic data only.

**What the binding proves:** that a holder of the account's wallet keys, through a host, bound the key to that product
account on that network. It does not prove who the person is, that the account is the seller the market says it is, or
that the host is honest: a modified host or anyone with the recovery phrase can bind any key. Only signing hosts can
sign or check bindings; a pairing host would need a new SSO request to its wallet, which does not exist.

**Who can read the values:** the provider app, the buyer's host, the recipient's host, and the person holding the
recipient account, who can copy them. Not the market's scripts on either side. This host is not a sandbox: do not open
hostile or same-origin products beside the demo. Read [Trust model](#trust-model).

**Simulation:** `http://localhost:5180/?field-demo=simulation` makes that tab's whole People Statement Store the
browser-local simulated one (`fixtures/statements/local-statement-node.ts`) and has the core grant allowances without
claiming them. Use the same host origin in both tabs. Nothing reaches a chain and no allowance is used. Without the flag
the market posts to the selected network through the core, which claims a real allowance; that path has not been run
since the bridge became generic.

**Limits:** the store keeps the latest listing and order per allowance account and channel, for one hour, so a seller
who is away can miss an order. A purchase's receipt draws the buyer's details only while the market document that made
it is open. No revocation, payments, contracts or multi-device recovery. Public statement metadata is not hidden.

Unit checks: `bun test tests/unit/locket tests/unit/field-image-plugin.test.ts tests/unit/boundaries.test.ts` (envelope,
recipient keys, the SVG drawing, the host's bridge with a stand-in binder and image service, the dev server's image
service over HTTP, the calls a product stops waiting for, and the import boundaries). The binding signature has Rust
tests: `cargo test -p truapi --lib field_key_binding`. Browser check, run by hand (CI does not run it):
`bun test browser/field-demo.spec.ts --timeout 300000`. It runs both providers, the market and two host tabs on the
simulated store, with its own Vite servers and caches, and refuses every request outside them.

When running another host dev server beside an existing one, give it a separate Vite `cacheDir`. Concurrent servers
sharing the dependency optimizer cache can fail with HTTP 504 `Outdated Optimize Dep`, before the core boots.

## Stacks and chains

Account → **Stack** selects the tab's Trinity stack. A switch reloads the tab.

| Stack         | TLD        |                                                  |
| ------------- | ---------- | ------------------------------------------------ |
| Paseo Next v2 | `.paseo`   | The default                                      |
| Previewnet    | `.testnet` | The product preview network                      |
| PCF Devnet    | `.dot`     | PCF's devnet on Paseo's chains, every chain RPC  |

- Each stack's chains, RPC nodes, TLD, DotNS resolver and services come from its descriptor. The build bundles the
  CLI's signed copies; a newer published copy is fetched after boot and used from the next load.
- **Add a stack** takes any descriptor URL or stack link. A link with a key pins its signer; a plain URL pins nobody.
  Custom stacks run every chain on RPC, without backup or username search, and may reuse any stack's TLD or chains.
- Asset Hub, People and Bulletin use their RPC nodes by default. The relay uses the embedded light client where the
  provider's catalog holds it.
- Account → Stack → **Chain connections** changes a chain's backend for the tab, applied on reload.
- The host trusts the answers of a chain on its RPC node, after asking each node which chain it serves.

Details: [docs/networks.md](docs/networks.md).

## How a product opens

A URL opens on its own origin. A name, such as `myapp.paseo`:

1. The host reads the DotNS `contenthash` of `app.<name>`, then `<name>`, from the Asset Hub RPC node.
2. The loader fetches the content from the Bulletin gateway, or from the build's Browse seed when the record matches.
3. From the gateway, it asks for one CAR stream. After a 2-second wait, it requests each missing block alone. The seed
   is read block by block.
4. It checks every block against its CID, unpacks the site, and serves it from a mount under the host's path.

The gateway is not trusted. The RPC node is. Details: [docs/opening-a-product.md](docs/opening-a-product.md). The dock
and Pocket: [docs/dock-and-pocket.md](docs/dock-and-pocket.md).

## Wallets and sessions

- Import a wallet from its BIP-39 phrase. The host creates none.
- Three shared test accounts (Alice, Bob, Carol) have public phrases.
- A session belongs to the tab. Storage is per wallet and product.
- **Backup** (prototype, on by default): the host keeps a sealed backup of a wallet's local state on the network, and a
  browser that signs in with the same wallet and holds nothing of it restores it, with no prompt. Not sync. See
  [docs/backup.md](docs/backup.md).

Details: [docs/wallets-and-sessions.md](docs/wallets-and-sessions.md).

## Chat

Host requests, Nova acceptances and conversation accounts can use guarded best-block posting on verified Paseo People
RPC connections. Backup and identity/contact security reads stay finalized. See [Allowance wait](docs/contact-chat.md#allowance-wait).
Nova request and acceptance confirmations resume after restart within their saved deadlines. Failed acceptances remain
visible in the Inbox for explicit retry, without undoing an established chat. See [Nova Chat](docs/nova-chat.md).
Malformed saved retry budgets pause only the affected request, without new claims or automatic retries.

- The Chat screen shows product rooms (TrUAPI Chat from product workers) and chat products over a draft bridge. It also
  holds the Inbox. See [docs/chat.md](docs/chat.md) and [docs/contacts.md](docs/contacts.md).
- **Contact Chat** is a chat product worker that the host ships and runs for every signed-in wallet, a prototype. Find a
  person by username in **+**, send a request, and they accept it in the Inbox. See
  [docs/contact-chat.md](docs/contact-chat.md).
- Contact Chat messages are plain text on the Statement Store. They are NOT ENCRYPTED. Only the setup is sealed.
- The first request is sealed with X25519 and the host proof is classical, so the recipient needs no published
  post-quantum key. Replies are sealed to a hybrid post-quantum X-Wing key by default.
- **Start over** (Chat options) sends a new request when a connection no longer gets through.
- The request service, sealing, delivery and the invitation demo: [docs/request-service.md](docs/request-service.md).
- **Kettle Chat** (dev server, or a build with `WEB_SIGNING_KETTLE_CHAT=1`; prototype) runs Kettle's MLS chat as a
  product of its own, alone or beside Contact Chat, for a signed-in wallet over the network or for the invitation demo
  over a simulated Statement Store. The developer tools' **Chat** tab (wherever Kettle Chat is) chooses which chat
  products run and which one New chat starts in. See
  [Kettle Chat](docs/contact-chat.md#kettle-chat-dev-server-prototype).
- **Nova Chat** (wherever the Chat tab is; prototype) talks to the official Nova apps' chat: their requests, acceptance
  and multi-device route. The host keeps the keys; the worker keeps the conversations. Not yet tested with an app on a
  phone. See [docs/nova-chat.md](docs/nova-chat.md).

## Developer tools

The Focus capsule shows or hides the tools: Account, Product, Preview, Inspector and More
([docs/developer-tools.md](docs/developer-tools.md)). The Inspector has six views
([docs/inspector.md](docs/inspector.md)):

| View       | Shows                                                   |
| ---------- | ------------------------------------------------------- |
| Activity   | What the host did                                       |
| TrUAPI     | Frames between product and core, decoded                |
| Workers    | Each product worker's state and console                 |
| Statements | Statement Store traffic that this tab saw, with sources |
| Chat       | Conversations that workers report about themselves      |
| Products   | Each mounted load: source, transport, size and outcome  |

## Tests

```bash
npm run typecheck      # tsc --noEmit
npm test               # unit tests (bun test)
npm run build          # tsc --noEmit && vite build
npm run test:browser   # headless Chrome; needs both WASM bundles and a Chromium
```

CI runs only `typecheck` and `npm test`. Run the build and browser tests by hand when the host or the core changes.
Specs and their scope: [docs/tests.md](docs/tests.md).
`bun test browser/guarded-posting.spec.ts --timeout 300000` checks best-ready chat posting with controlled chain
answers, real worker archives and an isolated browser clock. It sends nothing outside its loopback test server.

## Layout

Three owners, kept apart by `tests/unit/boundaries.test.ts`:

- `src/`: the host. `src/main.ts` is the composition root and `src/app/` the shell; `src/app/chat-products.ts` names the
  chat product every build bundles (Contact Chat).
  - `src/platform/`: stacks and their descriptors, network, storage, wallets, the core's Web Worker, Statement Store access.
  - `src/chat/`, `src/contacts/`, `src/products/`, `src/pocket/`, `src/notifications/`, `src/invitations/`,
    `src/backup/`: features. The host side of each bridge lives here too, such as `src/chat/draft/`.
  - `src/integrations/nova-chat/`: the host side of Nova Chat's own protocol (prototype): the key boundary that holds
    the user's keys, the request host, and `start.ts`, which the Chat tab's Nova Chat spec hands the host. Only the
    composition wires it in.
  - `src/locket/`: the host side of the draft Locket bridge: provider declarations, consent, the envelope, recipient
    keys, refs and the SVG a value is drawn as (`svg.ts`), loaded only on the dev server.
    `tools/demos/field-demo.ts` wires it, the dev server's image service and the explicit simulation;
    `field-label.ts` prints a label.
- `apps/`: the products the host runs, each in its own folder: `contact-chat/`, `kettle-chat/` (with Kettle's engine as
  WebAssembly in `wasm/`, built with `npm run build:kettle`), `nova-chat/`, and `field-demo/` (the provider and Small
  Market, served on separate origins by `field-products.config.ts`). `apps/shared/` is code the products share.
- `bridges/`: what both sides read: the draft contracts between the host and its products (`chat-v2/`, `host-requests/`,
  `nova-chat/`, `locket/`, `diagnostics.ts`, `worker-probe.ts`) and the pure byte, SCALE, hash and statement code they
  are written with (`primitives/`).
- `tools/`: developer tools. `tools/demos/` is dev-server only.
- `fixtures/`: synthetic data and test products. `tests/`, `browser/`: tests.
- `*-plugin.ts`: Vite plugins. `contact-chat-plugin.ts` builds the bundled chat products' workers.

Rules and the full map: [AGENTS.md](AGENTS.md), [docs/layout.md](docs/layout.md).

## Not supported

- Light-client-verified name resolution. The record's CID comes from one RPC node.
- Sharded directories in a website record. They are refused.
- Gating of a product's own network, device or media requests.
- Byte-range requests and non-UTF-8 HTML from a name's archive.
- Versioned hosted releases of this host, to test a product against a chosen TrUAPI version.
- A Coinage engine or a person-to-person chat of the host's own. Pocket has no balance and only the Humanity card.
- Payment callbacks (the core answers `Unsupported`) and live OS permission status.
- More than one product in front. Others run hidden behind it.
- Held products across a reload or a session change.
- Workers beyond Chat and the Humanity card. An address-bar product runs as `App`. A `Widget` runs only from the
  Humanity card.
- An external wire debugger. `VITE_TRUAPI_DEBUGGER_URL` connects but gets no frames: the signing core installs no debug
  tap. The in-page inspector does not need it.
