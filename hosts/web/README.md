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

## Two-product field demo (dev server only)

Two small independent SPAs test a field-provider boundary, not a canonical protocol. The provider app declares `address`
and `phone`; it runs as two separate providers on two origins, Field notebook and Travel notebook. Small Market requests
only `address`. The host registers declarations, asks the buyer once, in a sheet that shows the exact address and the
app it comes from (a choice when both providers hold one), and seals the address to the seller's separate X25519 key.
The address then appears inside the market's own packing slip and order confirmation, drawn by the host: the market
embeds a host-origin frame (`tools/demos/field-view.html`) where the address goes, and the host fills it. Neither
marketplace receives the address in its bridge replies, DOM, storage or frame text. The provider does know its own
values.

From `hosts/web`, run these in separate terminals, with the normal host prerequisites above already built:

```bash
npm run dev                       # host, localhost:5180
npm run dev:field-provider         # Field notebook, separate origin, localhost:5182
npm run dev:field-provider-2       # Travel notebook, separate origin, localhost:5185
npm run dev:field-market           # separate origin, localhost:5183
```

Only these exact origins and pages may use the field bridge (`FIELD_APPS` in `tools/demos/field-demo.ts`); any other URL
gets no bridge. The second provider is optional: with one, the flow below works the same.

Use two host tabs, not two standalone product tabs. In each host, sign in through **Account**. In **Product**, enter the
exact URL, leave the product id derived from the address, and select **Open**:

1. Alice opens `http://localhost:5182/provider.html`, enters a made-up address and phone and selects **Save to my
   Polkadot host**. No host sheet: see Host sheets below. Optionally she does the same with different made-up values in
   `http://localhost:5185/provider.html`.
2. Alice opens `http://localhost:5183/market.html` in the same host tab. Do not reload or sign out.
3. Bob opens `http://localhost:5183/market.html` in his host tab, goes to **Sell** and lists an item. His host sheet
   says where the listing goes and shows its full delivery key. Keep the key for comparison, then select **Publish**.
4. The listing appears in Alice's **Shop** within about 2 seconds. She opens the item card and selects **Buy now**. One
   host sheet shows the item, the exact address, the app it comes from and the seller's key; with two providers, each
   choice names its address and app. She compares the FULL key against Bob's host sheet and selects **Keys match, share
   address** (or the provider she wants). Her market opens the order confirmation, with her address drawn by her host.
5. Bob's **Orders** tab shows the order. Opening it shows the packing slip with the ship-to address drawn by his host.
   **Print shipping label** asks the host to print. Phone is never sent.

**Host sheets:** a sheet asks only when something leaves the host, spends a limited resource or is a real choice.

| Action                          | Sheet                                               | Why                                                                                                                                                                                                                                                |
| ------------------------------- | --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Provider registers (first save) | none                                                | Only apps the host made providers (`field-demo.ts`) can register; it discloses and spends nothing. The disclosure sheet later names the app and its verified id.                                                                                   |
| Provider saves values           | none                                                | The user typed them into that app; nothing leaves the host.                                                                                                                                                                                        |
| Seller lists an item            | **Publish "item"?**                                 | Posts publicly and, live, may use a daily posting slot. Shows the delivery key for manual pairing. Making the key is internal and asks nothing.                                                                                                    |
| Buyer buys                      | **Share your address with the seller?** (one sheet) | Discloses the address to one recipient and posts. Shows the exact address sealed, the app and its verified id, and the key to compare. With several providers, each choice names the address and app. The former separate provider picker is gone. |
| Viewing an order's address      | none                                                | The order was approved; the host still checks session, order and frame for every view.                                                                                                                                                             |
| Print shipping label            | none in the host                                    | The user asked; the browser's print dialog is the step.                                                                                                                                                                                            |

The address shown is read once, when the sheet opens, and is what gets sealed even if the provider saves a new value
meanwhile.

**How the host draws the address:** the market asks its bridge for a view of one order and gets a URL with a single-use
token, valid for 60 seconds. The host redeems the token only for a host-origin frame whose parent is the exact product
frame the token was issued to, then sends that frame the address over a `MessageChannel`. Replaying a used token,
nesting the frame one level deeper, opening it in another product or another host session shows "Address unavailable".
Ending the connection or the session blanks drawn views. This is the web form of the host-rendered names and images:
this host cannot intercept a product's image requests, so it uses a nested frame instead of an `<img>`. The product can
place and size the frame; the origin boundary keeps its text and pixels out of the product.

**Printing:** product frames are sandboxed without `allow-modals`, so a product cannot call `print()`. The host prints a
label it composes from its own records (order, listing, opened address) in a hidden host frame, and the reply to the
product says only that it printed. Closing the product or signing out removes that frame and prints nothing not yet
handed to the browser. The browser reports the dialog, not whether a printer printed.

**Transport:** the default is the selected network's real People-chain Statement Store, using the host's configured
connection, not a guessed endpoint. The core signs as Small market's own product account at a dedicated raw index; the
encryption key is random and separate. Posting claims one general Statement Store allowance and waits for finalized
allowance. Use disposable test-network accounts already registered with a free allowance slot. The host neither
registers nor funds them. Errors stop the operation and name the reason; there is no mock fallback. No username lookup
is needed. There are no contracts or payments.

Live check, 2026-11-10 on Paseo Next v2: the built-in Alice and Bob test accounts (`testalice.10`, `testbob.10`) are
lite people in ring 61 at the finalized head, so each can claim from its own 10 general slots per UTC day. Anyone
holding their public phrases can use those slots too. A listing reached the other host about 10 seconds after
**Publish**, and an order reached the seller about a second after it was placed (repeated after the UI redesign: 9 s and
1 s); both `statement_submit` calls answered `new` from the configured People RPC node. One earlier attempt posted
nothing within four minutes and was not diagnosed. Each run uses one slot per account per day.

Repeated later on 2026-11-10 with both providers, after the expiry and cache changes: the listing was posted 10 s after
**Publish** and seen by the buyer 4 s later; the order reached the seller 2 s after it was placed. The seller's slip
showed the chosen provider's address. Both `statement_submit` calls answered `new` from
`paseo-people-next-system-rpc.polkadot.io`.

**Offline boundary check:** explicitly open BOTH host tabs at `http://localhost:5180/?field-demo=simulation` and use the
built-in Alice and Bob test accounts. This selects the existing browser-local simulated Statement Store and skips real
allowance claims. The market shows **Simulated network**, and the host sheets' **Demo limits** say **SIMULATED**.
Product signing, encryption, the normal loader, provider bridge, consent and host-drawn views remain real. This mode
proves neither live delivery nor funding. The simulated store is this host origin's `localStorage`, so both tabs need
the same host address and port in the same browser profile. Use a fresh browser profile for a clean simulated board. The
flag controls this demo only, not the host's other services.

**Limits and trust:** this is web-only `web-field-demo/1`, not shared TrUAPI. iOS, Android and the Rust contracts are
unchanged on purpose. Recipient setup is manually trusted demo pairing, NOT verified host-custody attestation. A signed
listing, self-signed key or product-account signature cannot prove host-only custody. These fixture URLs use separate
origins, so the browser blocks their access to the host and provider DOM/storage; that does not make the whole web host
a sandbox. Do not open hostile or same-origin named products alongside this demo. Read [Trust model](#trust-model).

Values and private keys live only in host memory, outside product storage and backup. Reload/sign-out loses them;
reopening the marketplace in the same signed-in tab retains its recipient keys. No recovery, revocation, payments,
automatic updates or push. A store acceptance is not delivery proof. The store channels retain only the latest listing
and latest purchase per product account, with one-hour expiry, so a seller who was not online misses an earlier order.

What the open tab keeps:

- A listing leaves the shop when its statement expires, and **Buy now** refuses it, also when the sheet was answered
  after it expired. Statements that expire more than two hours ahead are not this demo's and are dropped.
- At most 50 listings from others. When full, an expired one makes room, or else the one that arrived first; a new
  listing is never the one dropped. The seller's own listings are always kept.
- A seller keeps only orders for its own listings that its key opens; other orders and noise take no room. At most 50,
  made room for the same way. The seller names an order by its id and its buyer, so another signer's copy of an id is a
  separate order and never replaces the first. Anyone can read a listing's key and order, so a full order list can push
  out the oldest real order.

Keep both sessions open for the one-listing, one-purchase flow, and use one tab per wallet: two tabs of one wallet post
as the same account without coordinating. Public statement metadata is not hidden or padded.

**Bridge contract** (`bridges/fields/protocol.ts`): failures carry a `FieldErrorCode` (`cancelled` when the user
dismisses a host sheet, `invalid`, `unavailable`, `limit`, `busy`, `network`, `session`, `unsupported`); unexpected
errors cross only as a generic message. The market's snapshot names its network as
`{ kind: "live" | "simulated", name }`. A provider declares a display name with its fields; host sheets always show that
name beside the verified product id, and `status` tells a provider which fields it saved, never their values. The
product chooses the view frame's size; the host view shrinks text to fit, so the address's length does not change
layout.

The host offers the bridge to each document after it loads (`web-field-demo-offer`), and the next load or closing the
product revokes it with its pending sheets and unused view tokens. The role comes from the exact URL the user opened
(origin and path, listed in `FIELD_APPS`) and the derived product id; a same-origin page the fixture navigates to keeps
that role. This is an opening policy for trusted fixtures, not verification of a hostile document. No protected values
go through the ordinary inspector's wire tap.

Focused unit checks: `bun test tests/unit/fields`. They cover provider choice, undeclared fields, denial, ciphertext
context, conflicting message ids, view-token binding, replay and expiry, revocation on session end, the printed label,
session reset, print cancellation, address-line limits, released views, cancellation during transport initialization,
listing expiry, full caches and which orders a seller keeps, and which received statements are read.

Browser check, run by hand (CI does not run it): `bun test browser/field-demo.spec.ts --timeout 300000`. It runs both
providers, the market and two host tabs on the simulated store, with its own Vite servers and caches, so running dev
servers are not disturbed. It refuses every request outside them and claims nothing.

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
  - `src/fields/`: the host side of the noncanonical web-only field demo: provider declarations, consent, sealing and
    host-drawn view tokens, loaded only on the dev server. `tools/demos/field-demo.ts` wires its live or explicit
    simulated transport; `field-view.*` is the host-origin view a product embeds and `field-label.ts` prints the
    shipping label.
- `apps/`: the products the host runs, each in its own folder: `contact-chat/`, `kettle-chat/` (with Kettle's engine as
  WebAssembly in `wasm/`, built with `npm run build:kettle`), `nova-chat/`, and `field-demo/` (the two independent demo
  SPAs, served on separate origins by `field-products.config.ts`). `apps/shared/` is code the chat products share.
- `bridges/`: what both sides read: the draft contracts between the host and its products (`chat-v2/`, `host-requests/`,
  `nova-chat/`, `fields/`, `diagnostics.ts`, `worker-probe.ts`) and the pure byte, SCALE, hash and statement code they
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
