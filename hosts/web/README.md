# TrUAPI Web Host

A TrUAPI host that runs in a browser tab and holds its own wallet. It embeds a product in an iframe, runs the core from
this repository in a Web Worker, and signs locally. There is no companion app and no QR pairing.

It is a tool for developers, run on a development machine or a private network, to try a product against the current
core. It is not a production host. The production web host is dotli (`hosts/dotli/`): it pairs with the Polkadot app,
which holds the keys, so the production browser core (`@parity/truapi-host/wasm/web`) has no signing host. This host
loads the `testing` core, built with `wasm-signing-host`, and keeps the recovery phrases it signs with in the browser.

## Trust model

The host trusts its user and the products the user opens. It is not a sandbox and makes no promise against a hostile
product.

- Wallets are disposable: recovery phrases are saved unencrypted in `localStorage`. Never import one that holds value.
- A product opened by name runs on the host's origin and can read the saved wallets through `window.parent`. Open only
  products you wrote or would open in a browser tab. A product opened by URL stays on its own origin.
- Separation by wallet is by storage name, not browser isolation. Permission prompts are a development aid, not custody.
- A product's own `fetch`, XHR, WebSocket, WebRTC and media follow the browser, not the core.
- Anyone who can reach a served host can use it. Serve it on loopback, or treat the page as public.

Details, prompts and differences from the native hosts: [docs/trust-model.md](docs/trust-model.md).

## Run it

Build the linked packages from the repository root, then start the dev server:

```bash
make codegen                                   # also builds @parity/truapi
npm run build --prefix js/packages/truapi-host
npm run build --prefix js/packages/truapi-debugger
make wasm                                      # both truapi-host bundles and truapi-provider
cd hosts/web && npm ci && npm run dev
```

Open <http://localhost:5180>, sign in, open the search card with the Docklet's middle button, type a product name or URL
and press Enter. The dev server answers `localhost` only; set `WEB_SIGNING_ALLOWED_HOSTS=name.example` (a comma list)
for another host name. `?product=<url>&productId=<id>` fills the address and product id but does not open them.

## Build and deploy

`npm run build` writes `dist/`, a plain static folder for any file server, such as GitHub Pages. Build for the path it
is served from:

```bash
npm run build -- --base /<repo>/   # https://<owner>.github.io/<repo>/
npm run build                      # served from the root
```

- A build made for `/repo/` and served at `/` loads no assets. Try one with `npx vite preview --base /<repo>/`.
- A build served from the root ships the Browse app's blocks as a seed. It reads Paseo Next v2 while it builds and fails
  if it cannot. `WEB_SIGNING_SEED_BROWSE=0` builds offline without the seed.
- Opening a name needs a secure context: `https` or `http://localhost`. A phone on `http://<LAN address>` opens URLs
  only.

More, including the dev server's username search route and phone setup: [docs/running.md](docs/running.md).

## Networks

Account → **Network** picks the tab's network before the core starts; a switch reloads the tab.

- **Paseo Next v2** (`.paseo`), the default.
- **PreviewNet** (`.testnet`), the product preview network.

Storage, history and the open product are kept per network. Chains are reached through the embedded light client; name
lookups and Asset Hub go through one public RPC node. See [docs/networks.md](docs/networks.md).

## How a product opens

A URL opens on its own origin. A name, such as `myapp.paseo`, opens like this:

1. The host reads the DotNS `contenthash` of `app.<name>`, then `<name>`, from Asset Hub over the network's RPC node.
2. The loader fetches the content from the network's Bulletin gateway or, for Browse when the record matches, from the
   build's seed on this origin.
3. From a gateway it asks for one CAR stream first. After 2 s, a block the stream has not brought yet is fetched by
   itself.
4. Every block is checked against its CID, the site is unpacked, and a service worker serves it from a mount under this
   host's path. The verified files are cached by content id.

The gateway is not trusted; the RPC node is. Limits, the seed, links and failures:
[docs/opening-a-product.md](docs/opening-a-product.md). The dock, Apps shelf and Pocket:
[docs/dock-and-pocket.md](docs/dock-and-pocket.md).

## Wallets and sessions

A wallet is imported from its BIP-39 phrase; the host creates none. The picker also lists three shared test accounts,
Alice, Bob and Carol, whose phrases are public. A session belongs to the tab, so two tabs can use two wallets. Core and
product storage are kept per wallet and product. See [docs/wallets-and-sessions.md](docs/wallets-and-sessions.md).

## Chat

The Chat screen shows product rooms (TrUAPI Chat from product workers) and chat products over the Chat Extension v2
draft bridge ([docs/chat.md](docs/chat.md)). Contacts and blocking: [docs/contacts.md](docs/contacts.md).

**Contact Chat** is the host's own chat worker, a prototype. It runs for every signed-in wallet
([docs/contact-chat.md](docs/contact-chat.md)):

- The network delivery is the default. Find a person by username in Chat's **+** (a build matches full usernames only),
  send a request, and they accept it in the Inbox.
- Messages are plain text on the Statement Store. They are NOT ENCRYPTED; only the setup is sealed.
- New requests use an X-Wing reply key by default; a dev server can choose X25519 instead.
- **Start over**, in a room's **Chat options**, sends a new request when a connection no longer gets through.

The **Inbox** ([docs/inbox.md](docs/inbox.md)) lists requests that wait for an answer. The invitation demo runs Contact
Chat as fixture users in tabs ([docs/invitations.md](docs/invitations.md)).

## Developer tools

The tools have five tabs: Account, Product, Preview, Inspector and More. The Focus capsule at the top of the app shows
or hides them ([docs/developer-tools.md](docs/developer-tools.md)). The Inspector has six views
([docs/inspector.md](docs/inspector.md)):

- **Activity:** what the host did, one row per event.
- **TrUAPI:** the frames between product and core, decoded.
- **Workers:** each product worker's state and console.
- **Statements:** the Statement Store traffic this tab saw, with a source for each fact.
- **Chat:** the conversations product workers report about themselves.
- **Products:** each mounted load: where it came from, transport, size and outcome.

## Tests

```bash
npm run typecheck      # tsc --noEmit
npm test               # unit tests under tests/unit/ (bun test)
npm run build          # tsc --noEmit && vite build
npm run test:browser   # headless Chrome, needs both WASM bundles and a Chromium
```

CI runs `typecheck` and `npm test` only. Run the build and browser tests by hand when the host or the core changes.
Install a Chromium with `npx playwright-core install chromium`, or set `CHROMIUM_PATH`. Browser specs in `browser/`:

- `smoke.spec.ts`: names, URLs, loading, the dock, product workers, contacts, chat products, Humanity.
- `build.spec.ts`: the static build, its Browse seed and Contact Chat archive.
- `mobile.spec.ts`: an emulated phone browser: gestures, zoom lock, full screen.
- `notifications.spec.ts`: the Inbox.
- `prompts.spec.ts`: the host's prompts.
- `app-area.spec.ts`: where a product in front sits.
- `docklet-search.spec.ts`: the Docklet's search.
- `keyboard.spec.ts`: an on-screen keyboard's visual viewport.
- `open-apps.spec.ts`: Browse, the Apps shelf and `navigateTo`.
- `preimage-image.spec.ts`: preimage lookups through the gateway.
- `workers-menu.spec.ts`: Product → Workers.
- `invitations.spec.ts`: demo users, test accounts and invitations.
- `contact-chat.spec.ts`: Contact Chat between two demo users.
- `contacts-pick.spec.ts`: `contacts.pick` for demo users.
- `chat-screen.spec.ts`: the Chat screen over its providers.
- `inspector-dock.spec.ts`, `workers-inspector.spec.ts`, `products-inspector.spec.ts`, `activity-log.spec.ts`,
  `statements-inspector.spec.ts`: the developer tools and inspector views.
- `wallet-delivery-live.spec.ts`, `chat-claims-live.spec.ts`: Paseo Next v2, only with `TRUAPI_LIVE=1`; they validate
  claims and submit nothing.

What each spec checks, and what is tried by hand: [docs/tests.md](docs/tests.md).

## Layout

Code lives with the feature that owns it. Tools, fixtures and tests sit outside the features. Per-file map:
[docs/layout.md](docs/layout.md).

- `src/main.ts`: composition root. `src/app/`: the shell (navigation, session, dock, search card, prompts, callbacks).
- `src/platform/`: network, storage, wallets, the core's Web Worker, Statement Store access, activity events.
- `src/chat/`, `src/contacts/`, `src/products/`, `src/pocket/`, `src/notifications/`, `src/invitations/`: features.
- `tools/`: developer tools; `tools/demos/` is dev-server only.
- `fixtures/`: synthetic data for tests and the dev server.
- `tests/unit/`, `tests/support/`: unit tests and helpers. `browser/`: browser tests.
- `*-plugin.ts`: Vite plugins for the loader, Contact Chat's archive, the Browse seed and username search.

## Not supported

- Light-client-verified name resolution. The record's CID comes from one public RPC node.
- Sharded directories in a website record. They are refused with a message, not opened partly.
- Gating of a product's own network, device or media requests. They follow the browser.
- Byte-range requests and non-UTF-8 HTML from a name's archive.
- Versioned hosted releases of this host, which would let a product be tested against a chosen TrUAPI version.
- A Coinage engine, and a person-to-person chat of the host's own: Chat shows product rooms and chat products only. The
  Pocket screen has no digital dollar balance, and holds only the pinned Humanity card. The host passes no payment
  callbacks, so the core answers those calls `Unsupported`. Live OS permission status is not provided either.
- Several products in front at once. One product is in front, and the others run hidden behind it.
- Keeping held products across a reload or a session change.
- Worker products beyond Chat and the Humanity card. A product opened in the address bar runs as `App`; a worker runs
  from Product → **Workers** for chat, or for Humanity's face, and a `Widget` runs only from the Humanity card.
- Dialling an external wire debugger. `VITE_TRUAPI_DEBUGGER_URL` connects and receives no frames, because the signing
  core's product runtime does not install the debug tap the dial reads. The in-page inspector does not depend on it.
