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

## Networks and chains

Account → **Network** selects the tab's network. A switch reloads the tab.

| Network       | TLD        |                             |
| ------------- | ---------- | --------------------------- |
| Paseo Next v2 | `.paseo`   | The default                 |
| PreviewNet    | `.testnet` | The product preview network |

- Asset Hub, People and Bulletin use their RPC nodes by default. The relay uses the embedded light client.
- Account → Network → **Chain connections** changes a chain's backend for the tab, applied on reload.
- The host trusts the answers of a chain on its RPC node.

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

Host requests can use guarded best-block posting on a verified Paseo People RPC connection. Conversation accounts,
Nova acceptances and Backup wait for finalized allowance. See [Allowance wait](docs/contact-chat.md#allowance-wait).
Nova request confirmations resume after restart within their saved deadline and retry budget. See [Nova Chat](docs/nova-chat.md).
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

## Layout

- `src/main.ts`: the composition root. `src/app/`: the shell.
- `src/platform/`: network, storage, wallets, the core's Web Worker, Statement Store access.
- `src/chat/`, `src/contacts/`, `src/products/`, `src/pocket/`, `src/notifications/`, `src/invitations/`, `src/backup/`:
  features.
- `tools/`: developer tools. `tools/demos/` is dev-server only.
- `fixtures/`: synthetic data. `tests/`, `browser/`: tests.
- `*-plugin.ts`: Vite plugins.
- `kettle-chat-wasm/`: Kettle's engine as WebAssembly, built with `npm run build:kettle`.

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
