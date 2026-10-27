# TrUAPI Web Host

A TrUAPI host that runs in a browser tab and holds its own wallet. It embeds a product in an iframe, runs the core from
this repository in a Web Worker, and signs locally. There is no companion app and no QR pairing.

It is a tool for developers, run on a development machine or a private network, to try a product against the current
core. It is not a production host. The dotli host at `hosts/dotli/` is the production web host: it pairs with the
Polkadot app, which holds the keys, and the production browser core (`@parity/truapi-host/wasm/web`) has no signing host
in it for that reason. This host loads the `testing` core, the bundle built with `wasm-signing-host`, and keeps the
recovery phrases it signs with in the browser.

## Trust model

This is a development tool. It trusts its user and the products the user chooses to open. It does not sandbox a product,
and it makes no security promise against a hostile one. Its separations are for keeping development data tidy.

- **Wallets are disposable.** Recovery phrases are saved unencrypted in `localStorage` on the host's origin. Any script
  on that origin can read them. Import a phrase you can lose, never one that holds value.
- **A product opened by name runs on the host's origin.** The host is one static site on one origin, such as GitHub
  Pages, so there is no second origin to give a product. The product's frame can reach the host page through
  `window.parent` and read the saved wallets. Open only products you wrote or would open in a browser tab. A product
  opened by URL stays on its own origin, and the host refuses to open its own origin that way.
- **Separation by wallet is by name, not by browser isolation.** The core's storage and the product's TrUAPI storage are
  kept under the active wallet and product id. See [Wallets and sessions](#wallets-and-sessions) for what the browser
  keeps apart and what it does not.
- **The permission prompts are a development aid.** They show what a product asks for and let you answer. They are not
  custody. Every host prompt (permissions, confirmations, the contact picker, the invitation prompt, chat controls) is
  drawn as the Polkadot app's bottom sheet, over the app's rectangle only and at its scale, so the developer tools stay
  visible beside it. It names the product that asks once, under its title. Escape or a tap outside the sheet gives the
  safe answer (deny, reject, cancel), and focus returns where it was.
- **A product's own requests follow the browser.** The host does not intercept a product's `fetch`, XHR, WebSocket,
  WebRTC or media. They follow the browser's rules and the target's CORS headers, and camera and microphone also need
  the browser's and the operating system's permission. Calls a product makes through TrUAPI, such as signing or a
  permission request, reach the core, which raises the host's prompts.
- **Anyone who can reach a served host can use it.** Serve it on loopback, or accept that the page is public and use a
  disposable wallet. The wallets are in each visitor's own browser, so a public copy exposes no one else's.

## Run it

The host links `@parity/truapi`, `@parity/truapi-debugger`, `@parity/truapi-host` and `@parity/truapi-provider` from
this tree, so it runs against the core at this commit. Build them first, from the repository root:

```bash
make codegen
npm run build --prefix js/packages/truapi-host
npm run build --prefix js/packages/truapi-debugger
make wasm
```

`make codegen` also builds `@parity/truapi`. `make wasm` builds both `truapi-host` WASM bundles and the
`truapi-provider` bundle. Then:

```bash
cd hosts/web
npm ci
npm run dev
```

Open <http://localhost:5180>, import a wallet, open the address card with the Docklet's middle button, type a product
URL and press Enter. To try it with the playground, run `yarn dev` in `playground/` and open `http://localhost:3000`.

The dev server answers `localhost` only. To reach it by another host name, such as a tunnel, start it with
`WEB_SIGNING_ALLOWED_HOSTS=name.example npm run dev` (a comma list). A static build needs no such setting.

The dev server also answers the identity backend's public username search on its own origin, because the backend sends
no CORS headers: `GET /__username-search?network=<id>&prefix=<text>[&limit=<n>][&cursor=<c>]`.
`username-search-plugin.ts` sends `prefix`, `limit` and `cursor` only to `/api/v1/usernames/search` on the
`identityBackend` of the network named, with no headers from the page, and passes the backend's answer and its errors on
as they are. Other parameters, other methods and requests from another machine are refused. A static build has no such
route.

`http://localhost:5180/?product=<url>&productId=<id>` fills in the address bar and product id. The product opens only
when you press Enter or choose **Open**.

## Networks

Account's **Network** control picks the network this tab runs on. It is chosen before the core starts and applied by
reloading the tab, so the core, the session and any open product are always built for one network; there is no switch
under a live page. The choice is kept per tab, so two tabs can run on different networks.

- **Paseo Next v2** (`.paseo`, the default) is where products are published for the current testnet.
- **PreviewNet** (`.testnet`) is the product preview network. Its DotNS names and content endpoints differ from Paseo's,
  so a name opened on one network is not found on the other.

A tab with no saved choice runs on Paseo, so tabs from before this control keep working. Storage, history, the username
learned for a wallet and the tab's own wallet and open product are all kept per network, so nothing from one network is
read on another. The recovery phrases are not per network, so the same wallet can be signed in on either. Switching
network reloads the tab, which then restores the wallet and product it had open on the newly chosen network, if any.

## Static build and GitHub Pages

`npm run build` writes `dist/`, a plain static folder. Any file server can serve it: no server code, no request-header
rules, and no extra host names or certificates. Opening a product by name needs a secure context, which is `https` or
`http://localhost`, and GitHub Pages is https.

The build is made for the path it is served from, given with Vite's `--base`:

```bash
# https://<owner>.github.io/<repo>/  (a GitHub Pages project site)
npm run build -- --base /<repo>/

# a custom domain, or a user site, served from the root
npm run build
```

Publish the contents of `dist/`. The build adds `.nojekyll`, so GitHub Pages serves every path.

`dist/` holds the host page and its assets, `sw.js`, the service worker for mounted products, and `truapi-sandbox/`, the
loader page and its script. They are ordinary files beside the host page, under the same base. To try the build locally
under a path, run `npx vite preview --base /<repo>/` and open `http://localhost:5180/<repo>/`.

The one thing the base must match is where the files are served. A build made for `/repo/` served at `/` loads no
assets.

## Opening a product

The address bar takes a web address or a product name.

| Opened as                     | Where the page runs                                            |
| ----------------------------- | -------------------------------------------------------------- |
| A name, such as `myapp.paseo` | A mount under this host's own path, served by a service worker |
| A URL                         | The URL's own origin                                           |

In both cases the product reaches the core through the public channel of `createIframeHost`. The frame keeps that
function's default `sandbox` of `allow-forms allow-same-origin allow-scripts`, so a product cannot open popups, use
`alert` or download files. The host adds `allow="camera; microphone"`, which lets the browser ask the user and grants
nothing by itself.

**Web addresses.** An address without a scheme gets `http://`. Only `http` and `https` are accepted. `localhost`, LAN
addresses and ordinary sites keep their meaning.

**Product names.** `myapp.paseo` opens the product published under that DotNS name. `http://myapp.paseo/` and
`polkadot://myapp.paseo` mean the same name, and a path, query and hash after the name are kept. A name is one label and
the TLD of the connected network, `paseo`. The core recognises `dot`, `paseo` and `testnet` as DotNS TLDs, so `.dot` and
`.testnet` names are refused with a message saying they belong to another network, and so are names with a port or
credentials or with more than one label. `https://myapp.paseo.li` is refused too: that public gateway serves the
Polkadot browser shell for every name and refuses to be embedded, so the message says to type the name.

**Bare names and recents.** A lone label such as `myapp`, alone or with a path, query or hash, opens as `myapp.paseo`,
and the field shows `.paseo` dimmed after it. Typing a dot, a port, a scheme or brackets means the text is complete as
written, so `localhost`, `localhost:3000`, `devbox:3000` and full URLs keep their meaning. A single-label LAN host
without a port needs `http://`. The completion applies when the address is opened, never to the field while you type.
Focusing the field lists the products this wallet opened, filtered as you type. An entry brings its own product id, or
none, and opens when chosen, never on focus. History is kept per wallet in this browser, holds addresses and ids only,
and skips addresses with credentials or secret-looking parameters (`token`, `key`, `code`, `session` and similar), which
still open as typed.

**Reset app data** in the Product section clears what the core stored for the open product under the signed-in wallet,
after a confirmation that names both. It closes the product, clears, and opens it again. It does not touch grants,
allowances, signing state, other products, other wallets or the page's own browser storage, and it cannot reach another
tab's memory, so close other tabs that use the same wallet and product first.

### How a name opens

On Open the name is looked up on the selected network's Asset Hub, its content is fetched from that network's Bulletin
IPFS gateway and checked, and it is served from a path of this host. The product id defaults to the name.

1. The host namehashes a record name and reads the DotNS content resolver's `contenthash` from Asset Hub over the
   network's Asset Hub RPC (for Paseo Next v2, `wss://paseo-asset-hub-next-rpc.polkadot.io`). It tries `app.<name>`
   first, then `<name>`. `app.<name>` is where `dotkit deploy` publishes the product's app executable, and the Polkadot
   browser reads it first too.
2. It fetches the first bytes of each CID from the gateway to see what it is. An app executable is a CARv1 archive of
   the site's directory, stored as one chunked UnixFS file. A website is a UnixFS directory. Both open. A record that is
   neither is skipped and listed.
3. It opens the loader, `<base>truapi-sandbox/index.html`, in the product frame. The loader registers a service worker
   for the mount `<base>product/<wallet>/<product>/<content>/`, fetches the blocks with `?format=raw`, checks each one
   against its CID, unpacks the site in memory and stores the files in Cache Storage under the content id. Then it moves
   the frame to the start path inside the mount. The worker serves the files as they are, with their type and no
   sniffing.

The mount path holds a hash of the wallet id, a readable form of the product id with its hash, and the content id. A
different wallet, product or content is a different mount, with its own worker. The same wallet opening the same content
again lands on the same mount.

**Links in the product.** A product built for the site root links `/assets/app.js`. On a project path that is outside
the mount, and a static server would answer 404. The worker answers every request its page makes, whatever the path, so
a root-relative link is read as a path in the archive. A relative link resolves under the mount. A request to another
origin goes to the network as it is. This does not reach a navigation the product starts to a root-relative document
(`<a href="/about">`, `location.assign("/x")`) or a reload of a page that moved there with `history.pushState`: the
frame leaves the mount. The page's `location.pathname` is the mount path, so a router that expects `/` needs hash
routing or a base path taken from the location.

Integrity, in two steps that should not be confused:

- **Bytes.** For an app archive, every chunk of the archive file is checked against the CID the record names, and every
  block inside the archive against its own CID. For a website, every block from the record's CID down. A gateway that
  sends anything else is refused, so the gateway is not trusted. An archive's inner root is not on chain, so the second
  check proves only that the site is consistent. The first is what ties it to the name. The cache is named by the
  content id and is written only after this check. The worker serves it without checking again.
- **The record.** The CID comes from one public RPC node, which is trusted as the source of what the name points at. It
  is not a light-client-verified read. Nothing proves that the CID is what the name's owner meant to publish.

The unpacker is written from the CARv1, dag-pb and UnixFS specifications. It refuses names that climb out of the site,
duplicate names, sharded directories, symlinks, files whose recorded sizes disagree, and content over these limits: 64
MiB of archive, 128 MiB of files counted once per reference, 10 000 files, 50 000 blocks, 4 MiB per block, 32 directory
levels and paths of 1 KiB. Loading also stops after a deadline.

While a product loads, one thin line runs along the top of the app, in the app's accent and under a device's top inset.
It moves without a percentage: the host knows the phase, not how much is done. The phase ("Looking up name…",
"Loading…") is announced to assistive technology, and the app's frame is `aria-busy`. The sandbox loader shows nothing
of its own, on the app's dark colour, so no white page and no address or CID show in the app; its steps are in the
Product tab and the log. A product page with no background of its own is white, as in a browser, once it loads. When the
loader fails, the app says **This app could not open** once, with **Details**, which opens the Product tab and its
reason, and **Close**. Closing or leaving a product ends its loading.

A failed lookup, or an address that cannot be opened, is shown in the address card and the Product tab and leaves the
open product as it is. The product is closed once the lookup succeeds and it is being opened, so a failure while the
archive is fetched, checked or unpacked is shown in the address card and the Product tab and leaves no product open.

The endpoints, TLD and content resolver address are the `paseo-next-v2` values in dotkit's `assets/envs.toml`, and they
match dotli's `packages/config/src/network.ts`.

A name needs a service worker, so it opens only from a secure page: `https`, or `http://localhost`. On another address,
such as `http://<LAN address>:5180` on a phone, it is refused before any lookup, with the reason in the address card. A
URL product opens on any address. The product frame is a secure context only when every page above it is.

A browser that does not offer service workers in the frame, or Web Locks, cannot open a name. An embedded web view often
lacks them: open the host in the system browser.

### The dock

A dock inside the product rectangle holds **Chat**, **Pocket**, **Browse** and **Apps**, as a phone host's tab bar does.
Browse is an ordinary product opened the way the address bar opens it. Chat and Pocket are screens the host draws
itself. On a local dev server Chat also holds the [Inbox](#inbox-prototype) prototype.

- **The Docklet.** As the Polkadot app's R1 Docklet: a glass dock at most 440 px wide with a round cutout, the app's 28
  px tab icons only, Chat and Pocket left of the Pill, Browse and Apps right of it, and a lens under the current one.
  The labels stay for screen readers. Apps shows the open count inside its glyph. It floats over the app, as in the app.
  Host screens and a product's frame end above it, the Pill included, so nothing of a product is under its glass: a
  product in a frame gets no inset it could use to keep clear by itself. The Pill opens the address card (see
  [Developer tools](#developer-tools)); while the card shows, the other icons step back.
- **Apps.** Its count is the products open in this tab, in front or running behind. It opens the Apps shelf, as the
  app's TabBarShelfView draws it: the bar's glass grows upward, with a row of icons on the right (the Inbox, when the
  host offers it, the core's state for this network, and **Settings**, which opens the developer tools' Account tab),
  and the open products under it as a grid of round icons. The one in front is marked. Choosing a product brings it in
  front; a long press, or its **×** on hover or focus, closes it. Each product shows how many Inbox items it sent that
  are not seen yet. The state icon is a grey ring whose dot is green when the core is ready and red when it failed, with
  the status line from Account as its label; there is no per-chain status. While the shelf is open the bar keeps its
  place, the Pill and the lens on the current tab; Apps gets no lens of its own. Escape or a tap outside closes the
  shelf.
- **An open product.** Two glass circles sit in a band over it, as in the iOS browser tab (`SPAViewLayout`): the
  product's frame starts below the band, which is the top inset (a device's 54 px, or the page's own
  `env(safe-area-inset-top)`), a 12 px gap, the 44 px circles and a 12 px gap. Nothing of the product is under the
  circles or a device's island, and the product is told of no inset, so it adds none of its own. The band does not fold
  away on scroll as the iOS one does: the host cannot see a product's scrolling. At the bottom the frame ends above the
  Docklet. Whether the iOS web view also ends above its tab bar was not settled from the vendored iOS source. On the
  left, **Minimize** for a product that can run behind, which every App can: it goes back to the last host screen and
  keeps running, and the shelf (or Browse, for Browse's own product) brings it back. A Pocket card's Widget cannot run
  behind and gets **Close** there instead. On the right, **More** opens a menu with **Product details** (the developer
  tools' Product tab) and **Close**.

- **What each opens.** On Paseo Next v2, Browse opens `browse.paseo`. An address opened while Browse is selected becomes
  its product, kept per network in this browser (`truapi-web-signing-host:dock:v1`, addresses only, none that carry a
  secret). An address typed over Chat or Pocket opens as a product of its own and leaves the screen. The Chat SPA is
  still a product: open `chat-spa-verbosity.paseo` in the address bar.
- **Products keep running behind.** Choosing another destination hides the product in front and keeps its frame, its
  channel to the core and its page state. The dock marks a destination with a green dot while its product runs. A hidden
  product can still raise a permission prompt, which names the product that asks. A reload, and any change of session
  (sign-in, sign-out, a wallet switch, the first username sync for a wallet), ends every held product, because a product
  belongs to the session that opened it. Only the one in front is opened again.
- **Opening one product from another.** A product's `navigateTo` for a name of this network, as `https://name.tld`,
  `polkadot://name.tld` or `polkadotapp://name.tld`, opens that product in front of the destination it came from. The
  destination keeps running, and so does the product opened this way: it joins the Apps shelf, in the order products
  were opened, and runs behind other screens until it is closed from the shelf or More. An address typed in the search
  field over a host screen or such an app opens an app of its own the same way. A name that is already a destination's
  product resumes that destination, and one already open as an app comes in front as it was left, as the iOS host does:
  no second copy starts, and its page does not change. Any other `http` or `https` address goes to the browser, and
  anything else is refused.
- **The Pocket screen.** The host draws it, laid out as the native Pocket is (the Android `PocketScreen`): a large title
  over two stacked cards, the digital dollar and then Humanity. The host itself reads nothing and signs nothing for it.
  - **Humanity:** the card the native host pins, backed by the personhood product `peopl.<tld>` (`peopl.paseo` on
    Paseo). See [Humanity](#humanity).
  - **Digital dollar:** the native card shows the balance of the wallet's Coinage coins. This host has no Coinage engine
    yet, so the amount slot reads "Unavailable" and says why on hover. No amount is shown, and nothing stands in for
    one. It takes no press.
  - **Not here:** tilt shine, the collectibles strip, adding a card, and removing one.
  - The artwork and texture are the native drawables, and the fonts are bundled, so nothing is fetched. The two
    drawables come from the GPL-3.0 Polkadot Android app and keep that licence, not this repository's MIT licence; the
    Inter and Manrope subsets are under the SIL Open Font License 1.1. The notices sit beside the files in `src/assets/`
    and are served with the build under `licenses/`.
  - `payment.balanceSubscribe` still answers `Unsupported`, so a product sees no balance either. See
    [Not supported](#not-supported).
- **The Chat screen.** See [Chat](#chat).

### Humanity

The Humanity card follows the Pocket RFC (`docs/rfcs/pocket-modality.md`) and the native hosts. Its face is whatever the
personhood product draws. It is not a check of the wallet's personhood, and this host adds nothing to it.

- **The face.** The card shows the face bundled with the host, a title and "Proof of personhood", until a face arrives.
  The first time Pocket is shown in a session, the host reads `worker.peopl.<tld>`. When that worker serves Pocket
  (`includes.pocket`) and lists the `humanity` card in `pocket.cards`, the host runs it as described in
  [Product chats](#product-chats), and asks it to render `RenderContext::PocketCard { cardId: "humanity" }`. Each tree
  replaces the face. A press or an edit inside the face goes back to the worker as a renderer action on the same
  context. A worker that does not offer the card is not started for it, and the card keeps its face.
- **Retries.** The host asks a worker that has just started again every 250 ms for 10 seconds, while it boots. A stream
  that ends after that is opened again after 1 second, doubling up to 30 seconds. A worker that restarts gets a new
  stream.
- **The kept face.** The newest face is kept per network and wallet in `localStorage`
  (`truapi-web-signing-host:network:<id>:pocket-faces:<wallet>:v1`), at most every 2 seconds and once more when the
  session ends. The next session shows it at once, before the worker answers. A face over 256 KiB is drawn but not kept.
- **Lifetime.** The worker runs from the first time Pocket is shown in a session until the session ends: sign-out, a
  wallet or network switch, or a reload. The native hosts start it with the session; this host waits until the user
  looks at Pocket. If Chat lists the same product, one worker serves both.
- **Pressing the card** opens the product's Widget, in front of Pocket, at `/?card=humanity`. Which content runs as the
  Widget follows the iOS host (`ProductResolver` and `ResolvedProduct.contentId(for: .widget)`):
  - The base name's `manifest` text record is read first. When it is present and not a valid root manifest (`$v` 1, a
    `displayName`, a `description`, and an icon with a `cid` and a `format`), nothing opens and Pocket says why.
  - With a valid root manifest, `widget.peopl.<tld>` is read. A valid widget manifest (`$v` 1, `kind` `widget`, at least
    one `dimensions.height`) serves its own `contenthash`.
  - A product that publishes no widget manifest, or no root manifest, has its base name's `contenthash` run as the
    Widget. `app.peopl.<tld>` is never read. On Paseo, `peopl.paseo` publishes no widget, so its base name's content
    runs.
  - When that record has no content, nothing opens and Pocket says why.
  - Stricter than iOS: a `widget.<name>` manifest that is present but not valid is refused with the reason. iOS skips it
    and serves the base name.

  The content is fetched and verified like a product opened by name. The frame runs as the product's `Widget` execution
  under the product id `peopl.<tld>`, so it shares the worker's and the app's storage, accounts and grants. Choosing
  Pocket in the dock ends the Widget. A session change, such as the first username sync, opens it again as the Widget. A
  reload does not open it again, and it is not added to the address history.

- **The core's Pocket calls.** A product's `pocket.listSubscribe` lists the pinned cards it backs, so `peopl.<tld>` sees
  `humanity` as privileged and every other product sees none. `pocket.removeCard` refuses a pinned card; the core's web
  bridge reports that refusal as `Unknown` with the reason, not as `Privileged`.
- **Not here yet.** The native hosts draw the face above an opened Widget, and the Widget can move it with
  `expandedCard.setFaceShown`. This host draws no face there, so that call answers `Unsupported`. The opened Widget is
  not kept loaded after it closes.
- **Trust.** The worker and the Widget run on this host's origin, as any product opened by name does (see
  [Trust model](#trust-model)).

### Chat providers

The Chat screen draws one list of conversations and one conversation view, and reads every chat through one small
interface, `ChatProvider` in `src/chat-provider.ts`. A provider owns its protocol, storage, delivery, retries and read
receipts; the screen only says what the user did and draws what the provider reports. The interface is this host's own,
not TrUAPI, and is meant to show the boundary a native host could copy.

- **What a provider gives.** Its conversations as a list kept in memory (title, preview, last activity, and an unread
  count when it keeps one), a conversation's history a page at a time (newest page first, oldest message first in a
  page, with an opaque cursor for the page before), what goes below the messages (a text input, nothing, or why the user
  cannot write), and events that say what changed. A message has a stable id, a sender with an id, a label and whether
  it is the user's, a status, reactions, and `Text`, `System`, `Custom` or `Attachments` content, among others.
- **What the screen gives back.** `send(conversation, text)`, `retry` of a failed message, and
  `markRead(conversation, message)` for the newest message whose end was actually on screen while the page was visible.
  Opening a conversation is not reading it. A provider without read state gets no `markRead` calls.
- **Optional parts.** Requests to accept or decline, starting a conversation, an account of its own, drawing `Custom`
  messages, contacts for the [contact picker](#contacts), the user's reactions (`react`, on messages marked
  `reactable`), blocking and leaving (`controls`, `block`, `unblock`, `blocked`, `leave`), files (`files`) and calls
  (`calls`). The screen shows each only for the providers that have it, and a disabled control says why.
- **Lifetime and stale replies.** `src/chat-inbox.ts` builds the providers when a session (wallet, network and identity)
  starts, and disposes all of them when it ends or changes. They run whichever screen or conversation is in front. The
  conversation view tags each read with its own token and the provider's epoch, and drops a reply that comes back after
  either changed: leaving the conversation, a new session, or a restarted source.
- **The providers.** `src/nova-chat-provider.ts` wraps the host's Nova engine ([Chat](#chat)) without changing it,
  including starting a chat by username and message requests. `src/product-room-provider.ts` reads the rooms product
  workers keep through TrUAPI Chat ([Product chats](#product-chats)), one provider per product.
  `src/chat-v2/provider.ts` reads the rooms a chat product keeps itself over the
  [Chat Extension v2 draft bridge](#chat-products-chat-extension-v2-draft-bridge), one provider per listed worker.

### Chat

Chat is an experimental, host-drawn screen. It is not a product and adds nothing to TrUAPI. It is written from reading
the Polkadot app's one-to-one protocol. It has received requests and control messages from the iOS app, but it is not
yet known to interoperate with the app in every respect.

- **Runs with the session.** Signing in starts chat for that wallet, network and identity, whichever screen is in front,
  and signing out, switching wallet or network, or reloading stops it. There is one engine and one connection per
  session; it reconnects with a growing delay when the connection drops. Messages arrive only while the tab is open.
  Accepting, declining, sending and retrying publish when the user does them, and received messages are acknowledged as
  they arrive. Chat does not run for the public development phrase, because anyone can hold that identity.
- **Keys.** At sign-in, the page derives the identity account (sr25519, `//product//uid.<suffix>/index(0)`) and the
  X25519 chat key from the wallet's entropy in `src/chat/keys.ts`, and stays off if either differs from what the core
  reports for the session. The keys and every key agreement live in the page, not in the core. The page is not isolated
  from a product opened by name on this origin (see [Trust model](#trust-model)), and the phrase they come from is
  stored unencrypted.
- **What it does.** It reads requests addressed to the identity, opens and verifies them (version 1, and version 2 with
  its identity proof), accepts or declines them, sends a version 1 request to a lite username or a `0x` account id, and
  exchanges text on the pairwise session with acknowledgements, one request outstanding per direction. A push token
  (kind 1) and device changes (kinds 17 and 18) are read and acknowledged but not shown or kept, as the apps do not show
  them. Reactions, leaving, blocking, files and calls are below. Other message kinds show as a placeholder with their
  kind number.
- **New chat.** **+** opens the host's [contact picker](#contacts), the one products open, titled **New chat** and
  naming no product. Choosing someone the user already chats with opens that conversation. Choosing anyone else opens
  **Send request to …** with an optional first message; only that button sends a version 1 request, through the
  provider's own start, with the person's account. Closing the picker leaves the screen as it was, and a choice made
  after the session changed is dropped. Only a provider that can start a chat (the host's Nova chat) offers **+**; a
  chat product run as a worker does not. There is no QR scanner, and an account id cannot be typed.
- **Reactions.** A reaction (kind 4) or its removal (kind 5) names a message and an emoji and is not a bubble. The host
  keeps one emoji per side and emoji on a message, as both apps key it. The user keeps one emoji per message, as the iOS
  app does: a new one is sent, then the old one is removed; the same one again removes it. Reactions are sent with the
  session's other unacknowledged messages until the peer acknowledges them, and one delivered again changes nothing.
- **Leaving.** Leave chat sends kind 13, then takes the conversation off the list and stops following its session. It is
  moved to the record's `left` list, not deleted. When the store does not keep the message, nothing changes. When the
  peer leaves, a line says so and the composer stops, as on Android; a new request from them reopens the conversation
  with its history, and kind 3 (contact added) clears the state.
- **Blocking.** Block is local, as on both apps: the person is not told, the conversation leaves the list and the
  [contact picker](#contacts), its session topic is not followed, so nothing they send is read or acknowledged, and a
  new request from them is set aside. History is kept. **Blocked** on the list unblocks; what they sent that the store
  still holds is read then, as on Android.
- **Files.** A file goes through HOP (`src/chat/hop.ts`), the app's encrypted handoff pool, not Bulletin: the page seals
  it under keys from a random 32-byte ticket (ChaCha20-Poly1305, whole up to 1,999,936 bytes, else 2,000,000-byte chunks
  and a root entry naming them), submits each piece to a HOP node over JSON-RPC (`hop_submit`) signed by the
  `//allowance//bulletin//chat` account, then sends a rich text message (kind 15) naming the root entry, the ticket, the
  node, the type, the size, and an image's width and height. The limit is 128 MiB. A received file is fetched only when
  the user presses **Download** or **Show image**, only from a node this host is configured with, and each piece is
  checked against its hash and its AEAD tag. Only a PNG, JPEG, GIF or WebP is shown; any other file is saved as plain
  bytes. Nothing is acknowledged (`hop_ack`), so the pool keeps the file for the sender's other devices. Tickets are
  kept with the message in `localStorage` and replaced with zeros in what the inspector keeps.
- **HOP nodes.** No network this host knows publishes its HOP nodes, so files stay off until a list of `wss://` URLs is
  set in `localStorage` under `truapi-web-signing-host:network:<id>:hop-nodes`. Uploads also need the
  `//allowance//bulletin//chat` account to hold a Bulletin storage authorization, which this host does not obtain.
- **Calls.** An audio or video call follows the apps' two-stage WebRTC setup (`src/chat/call-wire.ts`,
  `src/chat/call.ts`): the offer (kind 8), answer (kind 9) and further candidates (kind 10) carry the apps' compact form
  of the SDP, the receiver rebuilds the SDP from the apps' template, a data channel opens, then the caller adds audio
  and video and negotiates again over the channel. Kind 11 ends a call in any state. The camera and microphone open only
  when the user starts or accepts a call, and every track stops when the call ends, the provider stops, or the session
  changes. An offer older than 30 seconds, or one that arrives during another call, is answered with kind 11, as the iOS
  app does. An unanswered outgoing call is canceled after 60 seconds; the apps set no limit. The offer is the call's
  line in the conversation, which reads as outgoing, incoming, missed, declined or canceled, with the duration of an
  answered call.
- **ICE servers.** A call uses the JSON `RTCIceServer` list set under
  `truapi-web-signing-host:network:<id>:ice-servers`, none by default, plus TURN relay credentials when it can get them.
  A browser that hides its addresses behind mDNS names, as Chromium does until the page holds camera or microphone
  access, gives candidates the compact form cannot carry.
- **TURN relay credentials** (`src/chat/turn.ts`, `src/account-proofs.ts`). The native apps get them from the identity
  backend with a device-attested token, which a browser cannot make. This host uses the backend's proof route,
  `POST <identity backend>/api/v1/turn/issue-with-proof`, as the game product does. When a call starts or is accepted,
  after the media opens, the host asks the core, through TrUAPI `account.listRingVrfKeys` and
  `account.createAccountProof` on a provider for `chat.<tld>`, for a ring-VRF proof with Humanity's key (`peopl.<tld>`,
  Lite ring first) in the game's context (`dim2.<tld>`, suffix index 0), over
  `blake2b256("dub/turn-credential/v1" || timestamp as big-endian u64)`. The first ask may show the core's
  account-access prompt for `peopl.<tld>`. The identity backend comes from `paritytech/triangle-status`
  `environments.json`: `https://identity.dotspark.app` on Paseo Next v2, `https://identity-previewnet.dotspark.app` on
  PreviewNet. Credentials stay in memory until 30 seconds before they expire, are dropped when the session ends, and are
  never logged or captured; the call screen shows only why there is no relay.
- **Not working yet.** The core gives the proof only when Humanity's manifest and the game's manifest both grant `chat`
  the `context` scope, and today neither does, so the core answers `NotAllowlisted`, nothing is sent to the backend, and
  the call says why and goes on without a relay. After any refusal the host asks again only after 1 minute, or 10
  minutes for a missing grant. The PreviewNet backend lists `dim2.paseo,dim2.test` as accepted products, not
  `dim2.testnet`, so a PreviewNet proof would be refused there even with the grants. No proof has been redeemed and no
  relay has been tested.
- **Where keys come from.** A peer's chat key and username are read from Asset Hub `DotnsGateway.AccountNames` and from
  the People chain `Resources.Consumers` (its `identifier_key`), and a username's account from `LiteLabelOwner` and
  `Resources.UsernameOwnerOf`. App builds that register through the People chain leave no Asset Hub record, so either is
  used. When both name a key or an account and they differ, neither is used. A stored request whose sender had no key is
  looked up again each time the store delivers it. The identity's own allowance is the People chain
  `:statement_allowance:` entry.
- **One device.** The host announces no device key, so a peer would talk to it pairwise. It accepts with the older
  `ChatAccepted` message (kind 14), not the app's current `DeviceChatAccepted` (kind 20). Whether current app builds
  treat it as acceptance is not verified. It does not open multi-device envelopes, pair with a phone, sync history or
  share an identity with another device.
- **Storage.** Conversations are kept in this browser's `localStorage` per network and wallet
  (`truapi-web-signing-host:network:<id>:chat:<wallet>:v1`).
- **Writes.** Published statements are signed by the identity and need a statement allowance. Requests are signed and
  published by the identity itself, not by the anonymous per-request account the app uses when it holds a notification
  slot.
- **What is tested.** Checked against an outside source: the chat key against the core's pinned Rust value, the identity
  account and chat key against the running core, and the Asset Hub keys and value layout against live storage. Tested
  only against this code: the request, message, session and statement codecs and the engine, by round trips between two
  copies of it. The reaction, leave, attachment and call message layouts, the HOP root entry and the compact SDP are
  pinned to byte layouts written from the app sources, and the HOP root entry to the iOS app's own test vectors. HOP
  transfers run against a node in memory and calls against a second call manager in the same page. No captured app
  traffic, no live HOP node, and no exchange or call with a Polkadot app.

### Product chats

A product can chat through its worker executable, as a game product's weekly bot does. The Chat screen lists the rooms
workers keep in the same list as the person-to-person conversations above. These rooms are TrUAPI Chat: the worker's
calls reach the core as frames, which the inspector's **TrUAPI** view decodes.

- **Adding a worker.** Product → **Workers** takes a product name, such as `myapp.paseo`, and lists the signed-in
  wallet's workers with their state and **Remove**. The host reads `worker.<name>`: its `executable` text record must be
  a worker manifest (`$v` 1, `kind` `worker`, an `entrypoint` module inside the directory) whose `includes.chat` is
  true, and its `contenthash` locates the files. The archive is fetched and verified like a product opened by name, and
  the entry module is loaded in a hidden frame under the product's mount, connected to the core as the product's
  `Worker` execution. The product id is the name, the one its app opens under, so the two share the product's storage
  and accounts.
- **Lifetime.** The list of workers is kept per network and wallet. A worker runs while its wallet is signed in and the
  tab is open, whichever screen is in front. Signing out, switching wallet or network, or a reload stops every worker
  and ends its frames and render streams; signing in starts the saved ones. **Remove** stops a worker and takes it off
  the list; the rooms it made stay.
- **Default workers.** `ProductWorkers` takes a `defaults` list per network: workers the host adds for every wallet,
  before any the user adds, as the native app places a game's room by default. They run like any other worker and have
  no **Remove**. This host lists none, so no live worker starts without the user adding it.
- **Rooms and messages.** The host serves TrUAPI Chat (`src/product-chat.ts`): a worker creates rooms, posts every
  message kind the contract has, and sets each room's footer. `registerBot` is refused, as the iOS host refuses it.
  Rooms and the last 200 messages of each are kept per network and wallet in `localStorage`
  (`truapi-web-signing-host:network:<id>:product-chat:<wallet>:v1`). The core screens what a worker sends before it
  reaches the host; the host only stores and draws it. The iOS and Android hosts refuse every kind other than `Text` and
  `Custom`.
- **Rich text, files and buttons.** Rich text is its text and its media, each shown as an image when it is an `https`
  URL or an inline raster image. A file is its name, type, size and caption as the product gave them, unchecked, and a
  link that opens in a new tab with no referrer; only an `https` file is a link, and the host fetches none of it. An
  `Actions` message shows its buttons, in a column or a grid as it asks, and a press goes to the worker as an
  `ActionTriggered` chat action naming the message and the button, from the peer `native`.
- **Reactions.** A `Reaction` places the product's emoji on one of the room's messages and `ReactionRemoved` takes it
  off; it is not a message of its own. The product learns only its own messages' ids, so those are the ones it can react
  to, and a reaction naming any other id is refused. The user reacts to the product's messages with the same picker: the
  host sends `MessagePosted(Reaction)` from the peer `native`, naming the id `post_message` returned, and
  `MessagePosted(ReactionRemoved)` for the emoji it replaces. The user keeps one emoji per message. The user's own
  messages take none, because the product does not know their ids. Neither native host does this yet: both refuse
  reactions in product rooms.
- **Custom messages.** A custom message is drawn by asking the worker to render it (`RenderContext::ChatMessage`); each
  tree the worker sends replaces the card. `src/product-render.ts` draws the tree as DOM: text as text, never markup,
  sizes clamped, colors and type from the host's `--pr-*` tokens, which Chat and Pocket set to the same dark values.
  Images are shown from the worker's verified archive or from the network's Bulletin gateway by CID, and from no other
  address. An archive image is read from the copy in the worker frame's own storage: that frame is `credentialless` in
  Chrome, so the page's storage does not hold it, and the worker can write that copy too. A button press and a text
  field change go back to the worker as renderer actions. A tree deeper than 48 or larger than 2000 nodes is not drawn.
  A text field loses focus when the worker sends a new tree.
- **The room's text input.** A room whose footer is `TextInput` shows a composer. What the user types is kept in the
  room and sent to the worker as a `MessagePosted` chat action from the peer `native`, as the iOS host names the user.
- **Trust.** A worker runs on this host's origin, as a product opened by name does, and is trusted like one: it can
  reach the page and the saved wallets. Its own requests follow the browser. Its notifications, permission requests and
  navigation go through the same callbacks as an app's.
- **Not served yet.** Input surfaces of a worker, worker demand from the core (a worker runs whenever its wallet is
  signed in), bot registration, and the core's `beginOperation` lifetime hints. TrUAPI Chat has no operation to block a
  room, leave or delete it, upload a file, or place a call, so product rooms offer none of them. Pocket is served for
  the pinned Humanity card only; see [Humanity](#humanity).

### Chat products (Chat Extension v2 draft bridge)

**Draft, not TrUAPI.** A page-local bridge that lets a product's worker speak the Chat Extension v2 HOST–PRODUCT
contract to this host, to try that contract before TrUAPI carries it. It is the next step of product rooms: the product
runs the same way, as a worker the ordinary loader runs, like any product worker, but it keeps its own history and the
host asks it for pages, where a TrUAPI Chat room has the host store what the product posts. The contract is the one on
`paritytech/trinity-user-agents` branch `lc/chat-product` at commit `3b2316285`: `rust/crates/truapi/src/api/chat.rs`,
`src/v01/chat.rs`, `api/call_media.rs`, `truapi-host/src/chat-queries.ts`, and the RFCs `chat-extension-v2.md`,
`chat-extension-v2-additions.md` and `call-media.md`. `src/chat-v2/contract.ts` copies its types in the shape the TrUAPI
codegen gives them. Nothing in this repository's Rust crates, codegen or packages changes.

- **How a product connects.** Only a product `Worker` whose manifest includes chat, run by the ordinary worker loader.
  Its frame posts `{ type: "truapi-chat-v2-draft", protocol: "truapi-chat-v2-draft/0" }` to the host page with one
  `MessagePort`; the host takes it only from that worker's own frame and origin, binds it to that product id, and ends
  it when the worker stops or the session changes. A product uses `connectChatV2()` from `src/chat-v2/product.ts`, whose
  methods carry the contract's names (`createRoom`, `updateFeed`, `setRoomPreview`, `setRoomFeatures`, `setRoomCall`,
  `deleteRoom`, `capabilities`, `roomEventsSubscribe`, `onMessagesQuery`, `onUserAction`, `onParticipantsQuery`,
  `callMedia.start/signal/stop`).
- **Scoping.** The host, not the core, scopes the bridge: a product reaches only the rooms it declared through its own
  link, and its rooms are not TrUAPI Chat rooms. The core runs the worker as a `Worker` execution but sees none of the
  bridge's messages and grants nothing for them. The bridge carries no keys, entropy or grants.
- **A provider of its own.** `src/chat-v2/provider.ts` is one more `ChatProvider` per listed worker (id
  `chat-v2:<product>`), registered by `src/chat-inbox.ts`. It implements the provider interface as it is and leaves the
  Nova chat, the product rooms of TrUAPI Chat and the shared screen unchanged.
- **What the provider does with it.** A room that sets `productSource` takes its pages from `messages_query` (`Before`
  with the product's cursor as the user scrolls up). A sent message shows as pending under a local id, and the
  `MessageCreated` answer of `user_action` is drawn under the same id. Any refusal, `Rejected` included, leaves it
  failed with the reason and a retry. `Seen` goes out only when `reportSeen` is set. `update_feed` changes (`New`,
  `Edited`, reactions, `StatusChanged`) are drawn as changed messages; a `Deleted` makes the screen read the newest page
  again. `"user"` is the user and `"product"` the product; other participant ids are named through `participants_query`
  `ByIds`. `Notice` is a system row. The list draws the room's metadata, preview and unread badge. A user's file goes
  inline as a `RichText` media item up to 1 MiB; inline media opens from its bytes, and preimage, sealed and HOP media
  say this host does not fetch them. Queries use the RFC's time limits, a stopped or stale query is cancelled, and a
  worker restart is a new epoch that reloads the open room.
- **Calls.** A room that declares no `features.calls` has its call buttons disabled with the reason, and a button for a
  kind the room does not declare says so when pressed. A press sends `CallRequested`; Accept and Decline on a `Ringing`
  call send `CallAnswered`; Hang up on an `Active` call sends `CallEnded`. The call is the product's: it rings, places
  and ends it through `setRoomCall` and its own signaling. The media is the host's: `callMedia.start` runs only within
  180 s of a press or an accept in that room, and the host's signals reach the product as `CallSignal` room events,
  which it carries to the other side. This host has no real call engine for v2 rooms: `start` answers `Unsupported`,
  except for the local demo below, whose calls get `src/chat-v2/synthetic-media.ts`, an engine that opens no device and
  trades made-up signals.
- **Capabilities** lists `product_source_rooms`, `participants`, `inline_attachments` and `call_buttons`, nothing else.
- **Where it departs from the reference**, because the provider interface stays as it is: a `Rejected` message stays
  failed instead of being removed; a `Deleted` message outside the newest page goes only when the room is opened again;
  the call buttons of a room without calls are disabled, not hidden. Each would need one optional addition to
  `src/chat-provider.ts` (a removed message id on the `messages` event, and an `offered` check on `CallSupport` and
  `FileTransfer`), which is not made. `Actions` buttons draw disabled and `Custom` bodies show their preview text.
- **Not covered.** Host-store v2 rooms (their composer says the draft host keeps no messages), `reactions_query`,
  `message_actions_query`, `search_query`, `open_link`, `room_calls` and the Media service, preimage and sealed
  attachments, bots, mute and archive, wakes, replies and edits by the user, `allowedReactions`, pinned ordering, rate
  limits and most field bounds. Declarations live in memory for the session and are lost on reload, where the RFC keeps
  them across restarts. When a worker stops, its rooms' calls and call media end. The bridge is not TrUAPI, so the
  inspector does not show it.
- **What runs it today.** No published product speaks the bridge. `browser/synthetic-chat-product.ts` is a synthetic
  chat product, with made-up people and messages and no network or peer protocol; it is not the Verbosity chat product.

#### Local demo

The dev server can run the synthetic chat product as a product worker, so the bridge can be tried by hand. Run
`npm run dev`, open `http://localhost:5180` and sign in with a wallet, then Developer tools → **More** → **Chat demo** →
**Start**. Its rooms (Ada, a group with Ada and Grace, and Lin) join the chat list, and it shows in **Workers** as added
by this host. **Stop** and **Restart** sit beside it; what it is stays folded under **About the chat demo**.

- **How it runs.** `chat-product-demo-plugin.ts`, a dev-server plugin that a build leaves out, builds
  `browser/chat-v2-product-worker.ts` into one module, packs it as an app archive, describes it at
  `__chat-product-demo/worker.json`, and serves that archive's blocks, and only those, at `/ipfs/<cid>`. The host
  (`src/chat-v2/local-demo.ts`) adds it as one of its own workers, so the ordinary loader fetches and checks the archive
  and the core runs it as a `Worker` execution. Its product id is `localhost:<port>`, a development id no published name
  can take. A restart builds it again, so an edit to the product shows after **Restart**.
- **Only this machine.** The page offers it only from a dev server reached as `localhost`, `127.0.0.1` or `[::1]`, and
  the plugin refuses a request from or to another machine.
- **What to try.** Paging up Ada's 120 messages, sending (pending, then sent, delivered and read, then Ada's answer),
  `reject me` (refused, stays failed with the reason), a message with `[fail]` in it (fails once, then **Retry**),
  unread counts on Lin cleared once the messages are on screen, reactions, a file up to 1 MiB sent inline, the group's
  named senders and its system row, and audio or video calls with Ada: a call you place, and one she rings through the
  product. Calls use the synthetic engine, which opens no camera or microphone and carries no sound or picture.
- **Lifetime.** Nothing is saved. **Stop**, signing out, or another wallet or network stops the worker and drops its
  rooms; a reload or the next session starts with it off. The product keeps its history in memory, so **Restart** or
  **Stop** loses what was sent. The synthetic call engine serves only the demo's product, and only while it runs.
- **What it needs.** Nothing outside this host after sign-in. Signing in still starts the wallet's Nova chat and its
  network as usual; the demo adds no network traffic.

#### Nova chat worker

**Draft, dev server only.** The host's own Nova-compatible chat engine (`src/chat/`) can run inside an ordinary product
worker instead of the host page, as a chat product would run it. Developer tools → **More** → **Chat engine** → **Nova
chat worker**. **Host (default)** switches back. The choice is kept per network and wallet.

- **One engine at a time.** The host's engine stops before the worker starts, and the worker and its identity link stop
  before the host's engine starts again. Both would otherwise acknowledge the same messages and replace each other's
  statements. A worker that fails to start leaves chat off; it never turns the host's engine back on by itself.
- **What runs in the worker.** `src/chat-worker/worker.ts`, built by `chat-product-demo-plugin.ts` into an app archive
  (`__chat-product-demo/nova-worker.json`) and run by the ordinary loader as a `Worker` execution, product id
  `localhost`. The engine owns the protocol, the history, delivery and acknowledgements. The Statement Store, chain
  reads and storage go through the product's own TrUAPI client (`src/chat-worker/truapi-ports.ts`):
  `statement_store.subscribe` and `submit` (the core asks the user once for `StatementSubmit`), `chain.*` for the
  directory, and `localStorage` for the history, which the core keeps per product, so it survives a worker restart.
- **What stays in the host.** The wallet's chat keys. The host lends their operations over a page-local identity bridge
  (`src/chat-worker/identity-bridge.ts`, draft, not TrUAPI): decrypt a request or session message, check an identity
  proof, give the pair's public session topics, and build, seal and sign a session statement or a chat request. No
  operation returns a secret key, an agreed secret or the entropy, and none signs bytes the caller chose. The worker
  does get plaintext: it is the chat. Only the `localhost` worker gets the bridge, only in worker mode, and only for the
  session it was granted in; any other frame that asks is refused.
- **Signing.** `statement_store.submit` encodes the `SignedStatement` again as the store's sorted field list with the
  proof unchanged (`signed_statement_to_scale` in the core), so the worker's statements carry the same bytes and the
  same identity signer as the host's engine would send. This was read in the core's code and checked offline in unit
  tests; it was not checked against a live node or a phone.
- **History.** The worker keeps its own. **Copy the host's history into the worker** offers one copy at that switch,
  only taken by a worker with no history yet; the host keeps its record. After that the two diverge: what one engine
  receives the other does not see.
- **The seam for other engines.** `src/chat-worker/conversations.ts` defines `ConversationEngine` (conversations, pages
  of text, send, retry, change events) and shows any such engine over the v2 draft bridge.
  `src/chat-worker/nova-conversations.ts` is the Nova implementation. An engine over libsignal or OpenMLS would bring
  its own keys and state and implement the same interface; the identity bridge is Nova's, not a general requirement.
- **Who a room is with, and starting a chat.** Draft additions to the v2 bridge, beside a contact rooms proposal
  (`setRoomContact` in `paritytech/trinity-user-agents` `lc/product-only-chat` at `081dc4e43`, unmerged):
  - `setRoomContact { roomId, contact: { account, state } }`: the worker says whom a room is with, by public account and
    state (`Outgoing`, `Incoming`, `Established`, `PeerLeft`). Unlike that proposal it carries no username: the host
    names and draws the person itself (`src/host-names.ts`: what the user chose in the picker, the host's own chat
    record, else the username the chain holds) and never tells the product. Only the chat product the user switched to
    may set a contact, so no other product can make a room look like someone the host knows.
  - `startConversation { contact, welcome? }`, a host query: **+** opens the host's picker, offering the people of the
    worker's rooms and username search. Choosing someone already in a room opens it; anyone else gets the request form,
    and only **Send request** asks the worker. `contact` is an opaque reference the host made for that one start
    (random, single use, ten minutes, ended with the worker's grant, refused for a blocked person). The worker resolves
    it with `resolveContact` on the identity bridge, because the Nova wire addresses, seals and signs the request to the
    person's account, and the engine sends the request. Two starts for one person send one request.
  - This is not `contacts.pick`: a TrUAPI contact handle names a person to the core, which resolves it only for a
    transaction, and gives the host no way to learn which handle it minted. A chat engine in a product that must address
    a peer needs a resolve-for-chat capability that does not exist yet; the reference stands in for it.
  - The people of the worker's rooms are offered in the host's own **+** only. They are not `ChatProvider.contacts`, so
    no product's `contacts.pick` offers them and none becomes a payment recipient.
- **What it does not do yet.** Text only. A chat request someone sent shows as their conversation, and the first reply
  accepts it; declining, blocking, reactions, files and calls stay with the host's engine. A failed message shows as
  failed without its reason, which the v2 draft's status update cannot carry. Without the history copy the worker starts
  with no contacts, so it follows no existing conversation until someone sends a new request. While chat runs in the
  worker, the contact picker other products open has no Nova contacts: the host does not take contacts from a product.

### Inbox (prototype)

**Prototype.** The **Inbox** lists requests that products send and that wait for the user's answer: a persistent list,
not a passing toast, and separate from the operating system's notification permission. Chat's **Chats | Inbox** tabs
open it in place of the conversation list, and the Apps shelf's **Inbox** opens the same view: Chat, at its Inbox tab.
Both read one list, so an item seen, dismissed or accepted in one shows so in the other. It holds chat invitations and
profile sharing requests, not every chat message. No sharing, invitation or contact API feeds it: its requests come from
sample data or from [test users](#test-users-prototype).

- **Where it shows.** Only on a dev server reached as `localhost`, `127.0.0.1` or `[::1]`, like the
  [local demo](#local-demo); a build leaves it out with both entries. Developer tools → **More** → **Add sample
  notifications** fills the list of the signed-in wallet or test user, and **Clear samples** removes them.
- **The look.** The native apps have no Inbox, so this one is built from native parts: one list whose rows follow the
  iOS Chat Requests list (avatar, name and time, the message on two lines, a dot while unseen, and the kind, the product
  it came through and expiry under it). There is no detail page, and an item is never drawn as a conversation.
- **Seen.** A row is seen once at least 60% of it shows on screen, while the Inbox and the page are visible. Opening the
  tab does not mark rows below the fold, and a page in a background tab marks nothing. The Inbox tab and the shelf's
  Inbox count listed rows not seen yet, whatever their state, and a dot on Apps shows while any are. A product in the
  shelf counts its own. Every count reads the same items.
- **Actions.** A request that waits has **Dismiss**, and **Accept** when the feature that owns it can accept it. Sample
  requests have no owner, so they have Dismiss only. Seen is not answered: a request with no action stays waiting until
  it expires, and then it has no actions.
  - **Dismiss** takes the request off this list on this device. It is not an answer: nothing is sent, the stored item
    keeps its state and its `dismissedAt`, it still expires, and a repeat delivery does not bring it back.
  - **Accept** is the user's intent, handed to the product that owns the request. The row shows **Accepting…** until the
    product answers: it accepts, and the row shows Accepted, or it refuses, does not answer within 30 seconds, or is
    closed, and the reason shows under the row with the request still waiting. The host shows the product only when the
    product asks; tapping an accepted row asks the product again. For a test user's invitation the product is Example
    Chat, which checks the invitation, replies through the host and asks to be shown, at its **Accepted by you** list.
    Accepted on the row means the recipient's product replied; it is not the sender's connection. Example Chat has no
    chat conversation, so no Chat request or conversation is opened.
- **Storage.** Per network and wallet, under `truapi-web-signing-host:network:<id>:notifications:<wallet>:v1`, or per
  network and test user. Written only once used and removed when empty. Waiting requests and history survive a reload.
  `src/notifications.ts` holds the list, which does not know where a request came from, and
  `src/notifications-screen.ts` the screen. The sample requests, from senders and products ending in `.example`, are in
  `src/dev-tools/sample-notifications.ts`, which adds and removes them by id.

### Test users (prototype)

**Prototype, with FIXTURE IDENTITIES and LOCAL SIMULATED DELIVERY.** A tab can act as a made-up user, Alice, Bob or
Carol, instead of a wallet, to try features that need more than one person. Open the host in one tab per user and choose
a different test user in each. The core signs a test user in with a public BIP-39 test vector, so products get a real
session: `contacts.pick` opens the host's own picker over that user's contacts and returns the core's handle. The first
feature that uses two users is a chat invitation over a proposed host request/reply service, after the v3 "Establish
Connection" design: the product gives an opaque payload, the hosts own the request, its return route and keys, and
authenticate both directions, and the products see only payloads and opaque handles. Nothing in it is an agreed API.

- **Try it.** In one tab: Developer tools → **More** → **Test user** → **Alice**. The tab reloads with **Test user ·
  Alice** over the app, "Signed in as Alice" in Settings and **Example Chat** in Browse. In a second tab choose **Bob**.
  - **Pick a contact** in Example Chat calls `contacts.pick` and lists the core's handle, never a name.
  - **New chat** → **Choose contact** opens the same host picker and gives Example Chat a fixture contact handle.
    **Invite** opens the host's **Send invitation** prompt, which names the person. Example Chat shows **Waiting**.
  - Bob's **Inbox** badge, on Chat's Inbox tab and in the Apps shelf, shows 1 until the row shows in his list.
    **Accept** hands the invitation payload to Bob's Example Chat, which checks it, makes both conversation topics and
    its response, and replies through its host. Bob's host seals the reply for Alice and posts it on the request's
    return topic. Alice's host authenticates it and hands the payload to Alice's Example Chat, which checks it and only
    then completes: **Connected**. **Dismiss** takes the row off Bob's list and sends nothing. Alice can **Cancel** a
    waiting invitation; otherwise it waits until its deadline. More → Log shows each step.
- **Developer controls.** **Restart Example Chat** opens the product again, which gets its state back from the host and
  its own storage. **Reset Alice** removes that test user's lists on this network. **None** returns the tab to wallets.
- **What a test user is.** A core session from a public test phrase, under its own storage scope, apart from every
  wallet's. None of a wallet session's services start: no Nova chat, product worker, Pocket or registration, and the
  host reads nothing on chain for it. The core still looks up the account's DotNS name on Asset Hub once, as it does at
  every sign-in. Sign-in and sign-out are off. The choice is kept for the tab, so a reload keeps it. Choosing a test
  user ends the tab's wallet session and leaves saved wallets and their data alone. Contacts are made up
  (`src/dev-tools/test-users.ts`): Alice has Bob and Carol, Bob and Carol have Alice. Lists are kept per network and
  test user, under `truapi-web-signing-host:network:<id>:test-user:<user>:notifications:v1`, `...:outbox:v2` (sent) and
  `...:received:v2` (received, and the sealed reply). The `v1` sent and received lists of the earlier prototype are left
  in place and no longer read; **Reset** removes them too. Example Chat keeps its protocol secrets and topics in its own
  TrUAPI local storage.
- **The proposed API** (`host-requests/1`, `src/invitations/contract.ts`), over a `MessagePort` the product hands over.
  It reuses TrUAPI's `ContactHandle` and `ContactPickOutcome`. Each call has an `id` and one `result`.
  - Sender: `pickContact` (answers a `ContactPickOutcome`, as `contacts.pick` does), `request` (contact handle, purpose,
    expiry, payload; answers a request handle), `complete` (only after an authenticated reply reached the product; stops
    the reposts; sends nothing) and `cancel`. The host pushes `outgoing` (status) and `response` (an authenticated
    reply's payload).
  - Recipient, only after the user chose Accept: the host pushes `acceptIntent` (request handle, sender handle,
    payload), and the product calls `reply` (request handle, payload), `refuse` or `open`.
  - Statuses are `waiting`, `completed`, `cancelled` and `expired`: there is no delivered, declined or accepted status
    for the sender.
- **Who owns what.** The sender's host makes the request id, the return topic `T_SETUP` and a per-request reply key
  pair, watches `T_SETUP` before it posts, and keeps the reply key's secret. The recipient's host posts its reply only
  on the `T_SETUP` the authenticated request names, never on a route from the payload. Products never see a request id,
  route, reply key, evidence or anyone's identity; the recipient's product does not get the purpose or the deadline.
- **What is real here.** X25519 + ChaCha20-Poly1305 sealing, and authentication evidence shaped as v3's `AuthEvidence`
  (`kind`, a 32-byte `identityMac`, a 64-byte sr25519 `messageSignature`). A request's evidence covers the version, both
  identities, the product, the purpose, the request id, the times, `T_SETUP`, the public reply key and the exact
  payload. A reply's covers the version, both identities, the product, the request id, the digest of the exact request
  and the exact payload. Kind and identity order keep one direction's evidence from checking as the other's. The
  sender's host checks a reply against the contact it chose, not a key the reply names. A refused reply does not use up
  the request. Example Chat's own check is a toy protocol (`src/dev-tools/example-chat/setup.ts`).
- **What is simulated, and not the design's answer.**
  - The contact handle is 32 bytes the dev tools mint per test user and person. It stands in for the `contacts.pick`
    handle: the core gives the host no way to resolve that handle outside a transaction, so sending to it needs a
    host-only core operation (resolve or check a handle).
  - The identity credential (`src/dev-tools/fixture-identity.ts`) is a reference profile over keys derived from public
    test user ids: the MAC is BLAKE2b-256 keyed with the two identities' static X25519 secret, and the signature is
    sr25519 over the kind, transcript and MAC. Which key a real host signs with (the native app's wallet/UID account or
    another), the encoding, the exact transcript, pseudonymous identities per contact and any PQ part are not decided.
    The directory that says a key is a person is the fixture, and it holds every other test user, so a host takes a
    request from any of them, not only from the contacts its picker lists.
  - Delivery (`src/dev-tools/local-delivery.ts`) keeps sealed posts in this origin's `localStorage`: requests under a
    route derived from the recipient's key, replies under their `T_SETUP`. Each side posts again every minute and each
    post lapses after five minutes; these are dev choices, separate from the request's deadline, which reposts never
    move. No notification account signs a post and no allowance is checked. The route is linkable and any script of this
    origin can read or post.
  - When the recipient resends its reply and when it stops is open in the design. Here the recipient's host posts the
    same sealed reply again on the delivery's cadence, and when it starts, until the request's deadline.
  - Example Chat would watch its conversation topics through TrUAPI's `statementStore`; a test user reaches no chain, so
    it only keeps them.
- **Code.** `src/invitations/` is the feature and holds no fixture: `contract.ts` what a product and its host say,
  `ports.ts` what the host needs (contacts, an identity credential, delivery), `binding.ts` the envelopes and the
  transcripts their evidence covers, `sealing.ts` the sealing, `invitation-service.ts` one user's host side, and
  `outbox.ts` and `received.ts` what it keeps. `src/dev-tools/` holds what only tests and the dev server use: fixture
  contacts and identities, the local delivery, the Developer section (`dev-tools.ts`), the test users, the sample
  notifications and Example Chat. `src/dev-tools/invitation-flow.test.ts` runs two and three users end to end. Only a
  dev server on this machine loads `src/dev-tools/`, so a build has none of it; the host's contact picker reads the test
  user's contacts in place of the chat contacts.

### Contacts

The host serves TrUAPI Contacts (`src/contacts.ts`): `contacts.pick` opens a picker the host draws, and the core turns
the person the user chose into a contact handle, as `docs/design/contacts-pick-end-to-end.md` describes. A product gets
the handle only, never the list, a name or an account, and decides itself what to do with it: a chat product might open
an invitation, a payment product a transfer. Choosing someone only answers the pick. No permission is asked: choosing
someone is the consent.

- **Who is offered.** The session's chat contacts, from the providers that offer them (`ChatProvider.contacts`). Today
  that is Nova chat: its accepted contacts, not requests either side has not accepted yet, as on the native hosts, and
  not people the user blocked or conversations the user left. Blocking or leaving tells the core, as any contact leaving
  does. Product rooms and chat products offer none, because their peers are what a product says they are, and a contact
  can become a payment recipient.
- **Outcomes.** The picker names the requesting product and lists each contact by the name the host knows them by, those
  chatted with most recently first (by their last message, where the provider keeps one), then the rest. A search field
  filters the list by name, ignoring case; a name no contact has shows "No contact by that name." Choosing one answers
  `Picked`; Cancel, Escape or a tap outside answers `Dismissed`. A session with no contacts still gets the picker, to
  find someone by username, so the host never answers `NoContacts`. A contact that leaves while the picker is open is
  not an answer.
- **Finding people by username.** What is typed is also looked up by username (`src/username-directory.ts`), once typing
  pauses. On a dev server on this machine it searches by prefix, a page of 20 at a time, through the dev server's
  [username search route](#run-it) for the tab's network. Elsewhere, as in a build, there is no index, and what is typed
  is read on chain as a whole username (Asset Hub and People), so a person is found only by their exact username. A test
  user searches the other test users instead, so neither is asked. People found who are neither contacts nor blocked
  show under **Not in your contacts**, by username, with Show more for the next page. Choosing one from the index checks
  first that the username names the same account on chain, since the index is a listing and not a proof; if it does not,
  the sheet says so and nothing is picked. Then the pick answers `Picked` as for a contact. The person is not saved
  anywhere and does not become a contact. Text that cannot be a username is not sent, a newer query cancels the one in
  flight, closing the picker cancels it, and after the index asks to wait nothing is sent until the wait is over. A
  failure says why and keeps what was found.
- **Resolving handles.** When a transaction names a handle, the core asks the host which contact it hashes to
  (BLAKE2b-256 keyed with the session's handle key over the account) and checks the answer itself. The host tells the
  core when a contact leaves, including when the session ends, so a handle it cached stops resolving. A person picked by
  username is remembered in this page only, so their handle keeps resolving after the core forgets its cache; it names
  nobody after a reload, a session change, or once the user blocks them, and a transaction naming it then fails with
  `UnknownContact` until the product asks for a pick again. A blocked person is never picked.
- **Not served.** A contact list a product can read, labels for a picked handle, saving a person found by username,
  sending a request to a picked handle (test users use a fixture handle instead), and contacts from a worker's own chat.
  Paying a picked contact goes through the core's transaction path, which the browser test does not reach.

### Differences from the native hosts

These are accepted for a development host.

- The native hosts load `js/container`, which asks the core before a page's own `fetch`, XHR, WebSocket, WebRTC or media
  request. This host does not load it: those requests follow the browser and the target's CORS rules, and only explicit
  TrUAPI calls reach the core.
- Secure-context APIs are unavailable on plain HTTP, and native-only APIs are not emulated. Camera and microphone need a
  secure context, so on `http://<LAN address>` a product's capture call throws before any prompt. Elsewhere the
  browser's and the operating system's prompts apply.
- A product opened by name shares the host's origin, so the browser APIs that are global to an origin are shared with
  the host and its other products: see [Wallets and sessions](#wallets-and-sessions).

## Developer tools

The window has two zones: the app, on black, and the developer tools, a dark glass panel beside or under it. The only
developer control over the app is the **Focus capsule**, Dotli's 120 × 8 status capsule at the top centre. Its colour is
the core's state: grey while it starts, green signed in, amber signed out, red when it failed. It shows or hides the
tools at the tab they last showed; hidden, they give the app the whole window: Focus. A test user's marker sits at the
app's top left.

The tools' header holds the brand mark, **Developer**, a **Dev wallets** tag, the Right / Bottom switch and Hide. Under
it are five tabs:

- **Account:** the core's state, the network, the wallet (sign in, sign out, forget), Import a wallet and Account
  status.
- **Product:** Open a product (a name or URL, recent products, and the product id folded under them), Open now (its
  state, Close, Reset its data, What it may use, Details), and Workers.
- **Preview:** the app's size and its frame, described below.
- **Inspector:** the TrUAPI traces and the Chat diagnostics.
- **More:** Versions, the dev server's own controls (test users, sample Inbox items, the chat engine and the chat demo),
  and the Log.

**Search field.** The Docklet's middle button, the Pill, opens a card with one field, **Find a person or app**
(`src/search-card.ts`, `src/address-recents.ts`). Typing only edits the text and lists suggestions; it opens nothing and
picks no one. The field shows a dashed outline and a go button while it holds an address that is not open.

- **Apps.** The recent products that match. Enter with no suggestion chosen, the go button, or **Open** in the Product
  tab opens what is typed as a product, as before. When people show too, the Apps group starts with **Open** and the
  name or URL Enter would open, so a name that could be either reads as two choices.
- **People** (`src/search-people.ts`), while a wallet is signed in (not for a test user, and not signed out), and only
  for text that can be a username, so a URL looks no one up. First the session's contacts whose names match, then people
  found by username in the contact picker's own directory: the dev server's username index on this machine, else the
  chain by exact username. Blocked people are never listed. A person found by username is checked on chain first, as in
  the picker; until the check passes nothing is chosen, and typing again lets it go.
- **Choosing a person** opens Chat as New chat does (`ChatScreen.startWith`): a contact's conversation, or a request to
  someone new that the provider sends only on **Send request**, with focus in the message field. Nothing is given to a
  product. A choice made before the session changed is dropped. When Chat cannot start a conversation, such as for the
  public development phrase, people are still listed, and choosing one shows why under them instead, with no check on
  chain.

`Escape` closes the suggestions, then restores the address that is open and closes the card. When an address typed there
does not open, the card opens again with the reason. The chip inside the field shows the product id the open page runs
under. Scanning a code is not available in this host.

**Product id.** The Product tab's **Product id** field sets the id the core takes the page for. Leave it empty to derive
one from the address: `localhost:<port>` for a local server, the name itself for a product name, or the host name for a
named site. A bare IP address, such as a phone-reachable LAN URL, derives an id the core refuses, so enter one there. A
line under the field says which id the next Open will use and whether it was entered or derived. The override stays when
the address changes, until **Reset** clears it. Nothing reloads on edit or Reset; the id applies on Enter, the go button
or **Open**.

The id picks which product's accounts, grants, storage and privileges the page gets, including any privilege the core
gives that product, such as signing without a prompt. It is your choice and not a check that the page owns that id.
Enter only ids of products you are developing.

**Folds.** Import a wallet, Account status, Host permissions, product Details and Technical versions are collapsed.
Details holds what was read for a name: record, CID, origin, and what is and is not checked.

**Layout.**

- **Wide screens (over 900 px).** The tools open on the right (480 px wide) or at the bottom; the choice is kept in this
  browser. Their inner edge sets their size (`src/dock-resize.ts`): the left edge their width on the right, from 320 px
  to all but 360 px of the window, and the top edge their height at the bottom, from 160 px to all but 120 px. Drag it,
  or focus it and use the arrow keys (24 px a step), `Home` and `End`. Each size is kept in this browser apart from the
  other (`truapi-web-signing-host:devtools-width`, `...:devtools-height`), and a window that shrinks keeps the app its
  room. Resizing only refits the preview; the product is not reloaded. At the bottom the tabs sit in the header and a
  tab's groups sit side by side. Docked on the right, the inspector stacks its list over the detail.
- **Narrow screens.** The tools cover the app with a scrim: a 380 px panel on the right up to 900 px, and a bottom sheet
  with a handle on a phone. They start hidden. The app behind them is inert, `Escape` or the scrim closes them, and
  focus goes back to the capsule. **Open** closes them. The app fills the window, and the Preview tab says so.

Showing, hiding or moving the tools changes classes and sizes around the product frame. It does not move or re-create
the frame, so the product is not reloaded and the session is untouched.

**Preview.** The Preview tab sizes only the app's rectangle: **Auto** (Desktop while the app has 760 px or more, else
Phone), **Desktop**, which fills the app's zone, and **Phone**; **All sizes** adds Tablet, Desktop 1280 × 800 and
Custom. The choice stays when another tab shows. Two lines under it say what the app gets, at what scale, and the size
of the window and the tools; a set size also has a caption under it. The app lays out at exactly the chosen CSS size
(393 × 852 for Phone) and the rectangle is shrunk to fit; the window and the tools keep their own size. A set size has a
**Frame**: **Device** draws a bezel outside the size, **Round** rounds its corners and **No frame** leaves a plain
rectangle. On a phone size the device also draws the island and the home indicator over the screen. Host screens, the
Docklet and a product's frame keep clear of the island. The size is the screen's: a product in front gets less, and a
third line under the size controls says how much (`Product frame 393 × …`, and what the host keeps). No frame draws a
clock or status icons. A new tab starts on **Phone** with the device frame; the size and frame chosen are kept for the
tab. On a narrow screen the app fills the screen, whatever the choice, and the tab holds only a note.

**Inspector.** It is `createInAppDebugger` from `@parity/truapi-debugger`. The page taps the frames it relays between
the product and the core, in both directions, and the debugger groups and decodes them. Decoding is gated on the core's
own wire schema: if it differs from the page bundle's, the inspector lists operations without decoding them and the log
says so. Frames stay in the tab, may carry sensitive payloads, and are held in memory only.

**Chat diagnostics.** The inspector's **Chat** tab lists what the host's chat engine does: start, stop, reconnects,
subscriptions, requests received and why one was set aside, key lookups and which chain named the key, session requests
with the kind number of each message, responses, and every submission with its purpose and the store's answer. Chat is
not a product, so none of this is a TrUAPI frame, and it is listed apart from the traces. The events are metadata only:
shortened accounts and ids, kinds, counts and error text with hex shortened. A subscription also lists, in full hex, the
statement-store topics it filters on, at most 32: the request inbox topic,
`blake2b-256("chat-request" ‖ identity account)`, and one session topic per contact. A topic is a hash every statement
on it carries in public, not a key. An event about a statement names it by `blake2b-256` of its encoded bytes, the hash
the store gives it, and counts each arrival, so a replay shows as a repeat. The events never hold keys, shared secrets,
message text, push tokens or decrypted payloads. The last 300 are kept in memory and cleared when the session changes.

**Chat payloads.** The tab's **payloads** menu keeps what chat sends and receives, from the moment it is chosen, for
debugging the protocol. It is off by default and turns off again when the session changes. Nothing is kept while it is
off, so earlier events stay metadata only. An event's **Transport** part shows three layers:

- **Raw**, public: each JSON-RPC text the statement store sent or received, exactly as it went to or came from the
  connection, and the encoded signed statement. A frame is matched to a statement by the hash of the statement bytes it
  carries, and an answer to its request by connection and JSON-RPC id.
- **Decrypted**, sensitive and kept only at **wire + decrypted**: the statement's data as decryption returned it, or as
  the engine sealed it. It holds message text, push tokens and device keys. When there is none, it says why, such as a
  statement that does not decrypt.
- **Decoded**, derived, in two parts that copy apart or together. **Protocol fields** are the values read from the
  statement and the plaintext, in wire order. The binary has no names, so keys are the schema's field names, from the
  statement store's `Statement` and the Polkadot app's chat types; enum indexes, option tags, list counts and length
  prefixes are kept as read, so a push token's `platform` is `1` and a device's keys are `Vec<u8>` with their `length`.
  **Inspector annotations** are what this host adds, keyed by the protocol path they describe: names such as `Token` or
  `IOS`, ISO times, the statement hash, sizes, offsets, notes, and where and why reading stopped, with the bytes it did
  not read. Rich text, replies and other kinds with a plain layout are read as the schema lays them out; a kind the host
  cannot read stops there, with its bytes.

Each layer copies as text, hex or JSON. Bytes come only from the store's JSON-RPC tap, the statements the engine
receives or submits, and their plaintext, never from keys or the secrets derived from them. They are held in memory in
this tab, at most 300 frames, 300 statements and 4 MB, with a frame or statement cut at 256 KB, and the status line
counts what is evicted or cut. Lowering the level drops what it no longer keeps.

**Versions.** The menu shows client, host and core, provider and debugger versions, read from the linked packages and
the loaded `testing` bundle. A warning appears only when the core's wire schema differs from the client's. **Technical
details** holds the SHA-256 of the served `.wasm` files, the wire schema, and the source commit with whether the tree
has uncommitted changes. Facts that cannot be read show as `unknown`. Build-time values are measured when the dev server
starts or the build runs, so restart the dev server after rebuilding a WASM bundle.

A tab that opens a product opens it again after a reload, once its wallet is restored. A `?product=` link only fills the
form.

## Wallets and sessions

A wallet is **imported** from its BIP-39 recovery phrase. This host does not create wallets, and it has no built-in dev
accounts. Making an account, registering it and attesting it is done by the account-creation project; import the
resulting phrase here. Importing gives the host the keys and nothing else: it does not register or attest the account,
and one phrase is one wallet, however often it is imported. A saved entry that is malformed or holds an invalid phrase
is left out of the picker and stays in storage untouched, and the log says how many. If the whole
`truapi-web-signing-host:wallets:v1` value is not a list, saving and forgetting are refused until you fix or delete it
in DevTools.

**Account status** in the Account tab has two groups. Everything in it is read-only and nothing polls.

- **Account** shows the keys and reads the signed-in wallet's standing from the People chain when you open the fold or
  press Refresh. It reads `Resources.Consumers` and `PeopleLite.LitePeople` for the identity account and for the root
  key, at the finalized block, through the host's light client. It shows the username and the Lite or Person
  credibility. "Not found" means only that no record exists for that account. The layouts follow the product SDK's
  committed People chain metadata, so a runtime that changed them shows "Unreadable". **Ring** rows show where
  `Members.Members` holds the ring key that the account's own `PeopleLite.LitePeople` record (lite), or
  `People.AccountToPersonalId` and `People.People` records (full), name: Onboarding, Included with its ring index, or
  Suspended, read on the same finalized block. "No Members entry" means a key is named and unlisted. "No ring key on
  record" means no record names one, which is not a finding that the person is outside the rings. The key is the
  chain's, not derived here, so the rows are a diagnostic and do not by themselves explain a signing failure.
- **Current product** shows, for the open product, the core's local renewal ledger and two rows read on Refresh.
  **Allocation recorded** is the ledger: it lists a product only after an allocation reached the chain, so it is
  history, not a check of the chain, and not remaining quota.
  - **Account 0** is the public key of the product's account at `DerivationIndex::Index(0)`. The page asks the core
    through the product API, `account.getAccount`, using the typed `@parity/truapi` client over a provider the runtime
    makes for that product (`createProvider`), which is disposed after the call. The core derives the key from the
    wallet's entropy and returns the public key only; the page derives nothing. A product asking for its own account is
    not reviewed and a signing host derives locally, so the call opens no prompt. That is read from the source, not
    observed.
  - **Asset Hub balance** reads `System.Account` of account 0 at the finalized block through this host's light client.
    "No account entry" is a chain fact and is not shown as zero; a failed read shows "Unavailable". Decimals and symbol
    come from the chain's `chainSpec_v1_properties`; when it does not answer, the amount is shown in its smallest unit.
    It is the native balance only. A product may use other accounts.

  Not shown: Statement Store and Bulletin state, and PGAS. Their allowance accounts are `//allowance//…` derivations,
  not product accounts, so `getAccount` does not return them, and no exported call does. PGAS needs its asset id and
  decimals from the runtime's metadata, which this page cannot decode without a new dependency.

The core gives a local session only the lite username it is activated with and never looks one up, so this host supplies
it. After sign-in it reads `Resources.Consumers` for the identity account, then the root key, once per account, and on
Refresh. When the record's lite username differs from the session's, the host activates the session again with it, which
closes and reopens the open product, and `account.getUserId` then answers with that name. The name is kept with the
wallet's public facts, so the next sign-in starts the session with it. A full username is not passed on: local
activation takes a lite one only. With no record the session has no username, and `account.getUserId` answers `Unknown`
("No primary username for this session"). A record that cannot be decoded leaves the session as it is.

A failed read changes nothing about the session.

A session belongs to the tab. Each tab runs its own core, so two tabs can be signed in with different wallets at the
same time. A reload restores the tab's wallet from `sessionStorage`.

**Storage is per wallet.** Open the same product with another wallet, in this tab or another, and it starts with that
wallet's own data. The same wallet gets its own data back.

- _Kept apart by this host, by name._ Core storage (grants, AutoSigning keys) and the product's TrUAPI storage live in
  `localStorage` under the active wallet's id and the product id. The mount path, its service worker and the product's
  frame are per wallet, product and content too, so two tabs never serve each other's pages. The device encryption key
  is the exception: the core requires it to outlive logout and per-user namespacing, so it is stored once per browser
  profile.
- _Shared on purpose._ Verified archive bytes are cached by content id and are the same for every wallet.
- _Not kept apart._ The browser does not separate a path from its neighbours on one origin, and this host does not wrap
  its storage APIs. In Chrome the product frame is `credentialless`, which gives the page its own temporary storage: its
  `localStorage`, Cache Storage and workers start empty on each open and are gone when the frame closes, so they neither
  reach other wallets nor outlive the open. A browser without `credentialless` gives the product the host's origin
  storage, shared by every wallet and product. Keep a product's lasting data in TrUAPI storage. Anything global to an
  origin, such as cookies, is shared in any browser, and the product can reach the host page's own storage through
  `window.parent`.

Switching wallets closes the product and opens it again under the new session, because a product runtime must not
outlive the session it was opened under.

## Opening it from a phone

Open the deployed https page on the phone (see [Static build](#static-build-and-github-pages)): a name needs a secure
context, and that works with no other setup.

For a local copy, `npm run dev -- --host` serves the host on every interface, and
`http://<this machine's LAN address>:5180` opens on a phone on the same network. A URL product opens there, served on
its own port with the same option, for example `yarn dev -H 0.0.0.0` in `playground/`, with its LAN URL as the product
URL. A name is refused on that address, because plain http over a LAN address is not a secure context. Camera and
microphone need a secure context too.

Web Workers, WebAssembly and `getRandomValues` work outside a secure context, and the host uses nothing else there. The
chain connections are `wss://` from the embedded light client.

## Chain access

Chains are reached through the embedded light client in `@parity/truapi-provider`, using its bundled `paseo-next-v2`
catalog. The genesis hashes the core is configured with and the chain set reported to products come from the same
catalog. The light client keeps no state between loads, so each tab syncs from the chain spec checkpoint when it starts.
A name lookup uses one public RPC node instead, as described above.

The core opens one connection per chain and runs every product's `chainHead_v1_follow` over it, along with some follows
of its own. One light-client connection holds two follows and refuses a third with "too many active follow
subscriptions", so with a worker running, an opened card's Widget could not follow the People chain and its reads never
started. `src/follow-spread.ts` gives the core one connection that opens each follow on a light-client connection with a
free slot, a new one when none has, and sends every `chainHead_v1_*` call that names a follow to the connection that
holds it. The connections share the light client's sync.

A product's preimage lookups, such as Browse's catalog and app icons, are read from the network's Bulletin gateway, as
the native hosts read them. `src/preimage.ts` fetches `/ipfs/<cid>` for the raw blake2b-256 CID of the key, and the core
checks the bytes against the key. A gateway that does not have it, or does not answer within 30 s, gives a miss.

## Tests

From `hosts/web`:

```bash
npm run typecheck   # tsc --noEmit
npm test            # unit tests under src/ (bun test)
npm run build       # tsc --noEmit && vite build
npm run test:browser
```

CI runs `npm run typecheck` and `npm test` in the `host-web` job of `.github/workflows/ci.yml`, after building the
packages this host links. It does not run `npm run build` or `npm run test:browser`, which need the signing core's WASM
bundle: run them by hand when the host or the core changes. These are best-effort gates for a development tool: they
catch a broken host, not a hostile product.

`npm run test:browser` is a headless-Chrome smoke test of the host page. It needs the linked packages and both WASM
bundles built (see [Run it](#run-it)) and a Chromium: install one with `npx playwright-core install chromium`, or set
`CHROMIUM_PATH`. It starts the host with Vite on loopback, a counting target server and product fixtures, stands in for
Asset Hub and the Bulletin gateway inside the browser context, and signs in with a public test phrase. It uses no real
wallet and writes to no chain. The Chat check signs in with a synthetic phrase and closes every WebSocket the page
opens, so the light client reaches no chain. It checks that:

- a name opens from an archive under the host's own path, in a frame whose storage does not hold the host's wallets, and
  a root-relative script link in the product is answered from the archive;
- a product's own requests reach the target, counted there, because the host does not gate them;
- a URL product opens on its own origin, and the host refuses its own origin;
- a name held at the gateway shows only the line at the app's top, under its circles, on a dark loader with no address
  or CID, and the product when the gateway answers; a refused gateway shows the one failure with Details and Close; and
  closing the product while it loads ends the loading;
- opening, moving or hiding the developer tools, or resizing the product, does not reload it;
- an entered product id is shown as entered;
- the dock keeps Browse's product running while a host screen is in front, resumes it with its page state, and ends the
  held product when the wallet changes;
- Chat does not run or store anything for the public development phrase;
- Chat starts at sign-in while another screen is in front, with no chain connection, derives the same identity account
  and chat key as the core, has no on or off controls, and stops at sign-out;
- Pocket is drawn by the host as the native two cards, and gives the digital dollar no amount;
- a product built from `browser/contacts-product.ts`, with a synthetic wallet whose saved Nova chat holds two accepted
  contacts and one waiting request, and every socket closed: `contacts.pick` opens the host's picker naming the product
  and offering only the two accepted contacts; choosing one gives the product a 32-byte handle that is neither the
  account nor the name, the same handle again for the same person and another for the other; Cancel and Escape answer
  `Dismissed`; and a wallet with no contacts still gets the picker, with no one listed, and Escape answers `Dismissed`;
- a chat product's worker, built from `browser/chat-product-worker.ts`, with a synthetic wallet and every socket closed,
  runs through the same loader and core and records where TrUAPI stops it: its conversations show as rooms in the one
  list, but it has to push all its history and the host keeps the last 200 of 210 messages and never asks for a page;
  every message it posts shows as the product's, even those its own model says Ada, Grace or the user sent; what the
  user types reaches it with no message id, so the bubble stays `sent`; and it can open the contact picker and get a
  handle, which nothing turns into someone it can chat with;
- the local chat product demo, with a synthetic wallet, every socket closed and every request outside this host refused,
  starts from the Developer menu and runs through the dev server's archive, the same loader and the core, and speaks the
  Chat Extension v2 draft bridge: capabilities list only what the host serves; the list draws its rooms from their
  declarations without asking for messages; the host pages history from the product with its cursor; the user's own
  messages and named peers come from participant ids; a sent message becomes the product's stored message and moves to
  read through the feed, and the answer arrives as a new message under the id the bubble first had; `Seen` reaches it; a
  rejected message stays failed with its reason; an edit and a delete reach the open room; a `Notice` is a system row; a
  room without calls has its call button disabled; a failed message is retried by the product; and an outgoing and an
  incoming call connect through the product's own signaling, with the host's engine answering the other side's offer and
  making the offer when the user accepts; it makes no request outside this host; **Restart** gives a fresh product,
  **Stop** removes its worker frame and rooms, and signing out stops it and signing back in leaves it off;
- the Nova chat worker, with a synthetic wallet whose host chat holds one made-up contact, every socket closed and every
  request outside this host refused: the host's engine runs by default; switching to the worker with a history copy
  removes the host's conversation, runs the worker through the dev server's archive, the loader and the core, asks once
  for `StatementSubmit`, and shows the copied history in its room, named by the host while the worker declares the
  person by account only; **New chat** offers the worker's people, Cancel changes nothing and choosing Ada opens her
  room; a send reaches the core's statement store through `statement_store.submit` and fails there for want of peers;
  the synthetic demo's worker asks for the identity and is refused; switching back stops the worker and shows the host's
  conversation with its record unchanged;
- a product worker, built from `browser/chat-worker.ts` and served from a stood-in Asset Hub and gateway, runs from its
  manifest, creates a room, posts text and a custom card, draws an image from its archive in the card, redraws the card
  when its button is pressed, posts rich text with an inline image, a file link and buttons that all pass the core's
  screening, shows the text as text, the file as a link with no referrer, its own reaction and not the one it removed,
  receives the user's reaction through the core as `MessagePosted(Reaction)`, answers a press on its button for that
  message, answers what the user types, makes no request outside this host, shows in the inspector, stops at sign-out,
  is not seen by another wallet, and starts again when its wallet signs back in;
- the Humanity card, with every socket closed and every request outside this host, the stood-in Asset Hub and the
  gateway refused: a worker built from `browser/pocket-worker.ts` starts only once Pocket is shown, draws the face after
  it boots, and gets the face's button press; a press on the card opens the Widget built from
  `browser/pocket-widget.ts`, not the app, at `?card=humanity`, as a `Widget` execution with the worker's account and
  storage; choosing Pocket ends it; the face is kept for that wallet only and drawn at once on its next sign-in; and a
  worker that does not offer the card is not started; a product with no widget runs its base name's content as the
  Widget, not its app, and opens it again as the Widget after a wallet switch; and one with neither opens nothing and
  says why.

`browser/notifications.spec.ts` checks the Inbox prototype with synthetic phrases, every socket closed and every request
outside this host refused: nothing is stored until sample data is asked for, from the developer tools; the badge counts
unseen rows; a page in a background tab marks nothing seen; once visible, only the rows on screen are seen, a jump past
rows leaves them unseen, and scrolled into view they are seen; seen is not answered; sample requests offer Dismiss only
and no row opens; Dismiss takes a row off the list and keeps it stored, waiting, with its `dismissedAt`; a request
expires on the clock and loses its actions; Clear removes the demo's requests and their stored list; a reload keeps what
was seen and dismissed; another wallet sees none of it; signing out empties the screen and the badge; Chat's Inbox tab
and the Apps shelf's Inbox open the same list, the tab, the shelf, the dot on Apps and a product's own count clear
together once its rows are seen, while Apps keeps counting open products; the shelf takes focus when it opens and gives
it back to Apps on Escape, brings a product running behind to the front and closes it; and on a phone the four dock
entries fit, long rows wrap, Dismiss is a full tap target, and nothing scrolls sideways.

`browser/prompts.spec.ts` checks the host's prompts with a test user and a fixture product: in a phone preview the
dialog covers the app's rectangle only, with a dark sheet inside it, the developer tools outside it and no page-wide
backdrop; the picker names the product once and lists contacts by name only, with Cancel below; focus starts on the
title; a row, Escape and a tap on the scrim each answer; a camera permission keeps its detail and is denied; focus
returns where it was; a scaled preview draws the sheet at the app's scale; and on a phone browser nothing scrolls
sideways and the buttons are 44px or taller.

`browser/app-area.spec.ts` checks where a product in front sits, with a synthetic product on loopback that has a field
at its very top and a button fixed to its bottom, no wallet, every socket closed and every request outside this host and
the product refused: in the Phone preview with each frame, drawn at full size and shrunk, and in a phone-sized window,
the field starts below both circles and, in the device frame, below the island, and the button fixed to its bottom ends
above the Docklet and its Pill; the product reads no safe-area inset of its own; the Preview tab states the product's
frame and what the host keeps; More opens its menu from the keyboard, clear of the island and inside the app, Escape
gives focus back; and Minimize keeps the same product running until Browse brings it back.

`browser/docklet-search.spec.ts` checks the Docklet's search with a synthetic wallet whose saved chat holds one contact,
every socket closed, a synthetic answer on the dev server's username route and every other request outside this host and
a loopback product refused: the field reads **Find a person or app** with no scanning note; a name lists **Apps** (Open
and the name) apart from **People** (found by username, marked as not a contact), and typing opens and picks nothing;
the list stays inside the app above the field; a person found by username is checked first and, offline, is not chosen;
a contact chosen with the keyboard opens their chat with focus in the composer; `Escape` gives focus back to the Pill; a
URL looks no one up and Enter opens it; with the public development phrase, whose chat stays off, people are still
listed and choosing one says why; and signed out no people are offered. `browser/chat-screen.spec.ts` also checks that a
person chosen outside Chat opens a contact's chat or a request to someone new, sent only when asked.

`browser/open-apps.spec.ts` checks the products open in a tab, with three names served from one synthetic archive on a
fake Asset Hub and every socket closed: Browse's product opens another with `navigateTo`, and the shelf lists both with
the new one in front; going back to Browse keeps both and Browse's document; a third app keeps the other two; the shelf
brings an app back without a reload; opening an app that is already open brings it in front with no second frame;
Minimize keeps it in the shelf; Close in More ends only it; and signing out ends every app.

`browser/preimage-image.spec.ts` checks preimage lookups, with a synthetic archive on a fake Asset Hub, a stood-in
gateway and every socket closed: an image a product looks up by its preimage key, as Browse looks up an app's icon, is
read once from the gateway's `/ipfs/<cid>` path and drawn.

`browser/workers-menu.spec.ts` checks Product → Workers and More → Chat demo, with a synthetic phrase, every socket
closed and every request outside this host refused: signed out, nothing can be added or started; the Chat screen has no
Workers page or demo panel; a typed name survives hiding the developer tools, and an empty one is refused until edited;
Start runs the demo as a worker the host added, with no Remove, and its rooms in Chat; opening and closing the developer
tools restart no worker; Restart gives one new frame and Stop takes it and its rooms away; on a phone the sheet holds
the section, keeps a typed name across Escape, and nothing scrolls sideways; and a test user tab runs no workers.

`browser/invitations.spec.ts` checks test users in tabs of the normal host, with no wallet, Playwright's clock, every
socket closed and every request outside this host refused: a test user tab is marked and signed in to the core as that
user, with Example Chat in Browse and the Developer controls only in the developer tools; the host's Send invitation
prompt offers the user's contacts and Cancel sends nothing; an invitation shows a badge of 1 in the chosen recipient's
tab only, and showing in the list is seen, not answered; Accept reaches the sender's product, hands the recipient's
product the same setup data and then opens it, and the accepted row opens it again; Dismiss sends no answer and the
sender's invitation waits until it expires; a request expires on the clock on both sides, and its row loses its actions;
replays and late or contrary answers change nothing; restarting Example Chat restores it; a reload keeps the test user
and the lists; another network shows none of it. It also checks that a test user's session is not the saved wallet's and
leaves the wallet and its notifications alone, and that on a phone nothing scrolls sideways and the banner's buttons
fit.

`browser/contacts-pick.spec.ts` checks `contacts.pick` for test users: the host's picker names the product and lists the
user's two contacts; a pick gives the core's handle, the same for the same person and another for another; Cancel and
Escape give Dismissed; the product shows no name or account; a second product, opened by URL, gets the same handle for
the same person; another test user gets another handle for the same contact; the username search finds a test user who
is not a contact and shows them apart, lists a contact once, does not look up text that cannot be a username, gives the
product a handle for the person found, the same each time and without making them a contact, and never asks the username
index; and with no session the core refuses the pick with NotConnected and no picker is drawn.

Three narrower browser tests use no wallet. `browser/inspector-dock.spec.ts` loads the host page with every request
outside loopback refused, and checks that on a wide window the developer tools open on the right by default and move to
the bottom across the window, the Phone-sized app keeping its size and its device frame; that both inspector views fill
the tools, that their top edge sets their height at the bottom, that on the right their left edge sets their width by
drag and keys within its bounds, apart from the height, kept after a reload and without reloading the product, and that
the side is kept after a reload; that Focus gives the app the whole window and brings the tools back at the same view;
that the side switch is a segmented control of two 36 × 26 px pills, each inside its track, and Hide a 28 px circle,
icons centred, on either side, in either colour scheme and on hover and focus; that a narrow window keeps them a sheet
over the app; and that the Pill opens the search card with focus in the field, shows why an address cannot open, and on
`Escape` closes and gives focus back. `browser/chat-diagnostics.spec.ts` mounts the inspector's Chat view alone on
made-up events, and runs a chat exchange between two synthetic wallets over an in-memory JSON-RPC statement store to
check each payload layer, the copies, the capture levels, and that no key or secret shows. `browser/chat-screen.spec.ts`
mounts the Chat screen alone over the Nova and product room providers with stand-ins behind them and the chat product
provider with the synthetic chat product on the other side of the bridge in the page, synthetic data only and every
request outside this host refused, and checks one list across providers, paging on scroll, sender names and the user's
own messages, status changes, a retried message staying one message, a product room's custom card, buttons, reaction and
non-`https` file, the chat product answering while another room is open, unread cleared only for messages on screen, a
product restart and a new session. It checks New chat through the host's contact picker over a test username index:
Cancel changes nothing, a username that names another account on chain is not chosen, a contact opens their chat, anyone
else opens a request that the provider is asked to send only on Send, once, and a choice made after the session changed
is dropped. It also checks the user's reactions in a Nova conversation and a product room, blocking, unblocking and
leaving, a generated image sent and fetched through a HOP node held in the page, and audio and video calls over real
WebRTC to a second call manager in the same page, with generated media (a WebAudio tone and a canvas), not a camera or
microphone, and every track stopped when the call ends. The shared Chromium runs with `WebRtcHideLocalIpsWithMdns` off,
so its candidates carry addresses. Set `CHAT_SCREENSHOTS` to a directory to keep screenshots.

The smoke test runs against the Vite dev server. It does not show that a static host serves the build: open the built
`dist/` from the host you will use, as described in [Static build](#static-build-and-github-pages).

Not covered by automated checks, and tried by hand in Chrome when the behavior changes: real camera and microphone
capture, a call or file transfer with a Polkadot app, the phone setup, two tabs with different wallets, the build under
a project path, the inspector's rendering, and layout on narrow screens.

## Layout

- `src/main.ts`: the page and its wiring. `src/product.ts`: embedding a product and relaying its frames to the core.
  `src/callbacks.ts`, `src/prompt.ts`, `src/reviews.ts`: the host callbacks and prompts.
- `src/wallets.ts`, `src/storage.ts`: wallet and per-wallet, per-network storage. `src/network.ts`,
  `src/network-config.ts`, `src/network-choice.ts`: the light client, the networks it serves and the per-tab choice.
  `src/follow-spread.ts`: spreading the core's follows over light-client connections. `src/preimage.ts`: preimage
  lookups from the Bulletin gateway.
- `src/dotns.ts`, `src/address.ts`: name lookup and the address bar's grammar.
- `src/shell.ts`: the dock's destinations and saved addresses, and what a product's `navigateTo` turns into.
  `src/pocket.ts`, `src/pocket-screen.ts`: the Pocket cards, the Pocket the core's calls see, and how the screen draws
  them. `src/pocket-faces.ts`, `src/pocket-card-view.ts`: a card's kept and live faces, and drawing them on the card.
  `src/widget.ts`: finding the Widget an opened card runs.
- `src/chat/`: the chat engine. `gate.ts` refuses the public development phrase, `keys.ts` derives the keys,
  `identity.ts` gives the engine their operations and never the keys, `protocol.ts` and `statement.ts` are the wire,
  `statement-store.ts` the store's JSON-RPC, `directory.ts` the chain reads, `store.ts` the saved conversations and
  `engine.ts` ties them together, and `diagnostics.ts` holds its events. `hop.ts` moves files through HOP
  (`hop-memory.ts` is the node tests use), `call-wire.ts` is the call signaling, `call.ts` runs a call and `turn.ts`
  gets its relay credentials (`src/account-proofs.ts` asks the core for the proof). `src/call-view.ts` draws a call over
  every screen. `capture.ts` keeps payloads when the user asks, `inspect.ts` decodes them, and `memory-rpc.ts` is the
  in-memory store tests use. `src/chat-screen.ts` draws the Chat screen, `src/chat-diagnostics-view.ts` the inspector's
  Chat tab and `src/chat-transport-view.ts` an event's payload layers.
- `src/contacts.ts`: the contact picker and handle lookup.
- `src/username-directory.ts`: finding people by username for the contact picker.
- `src/username-search.ts`, `username-search-plugin.ts`: the dev server's username search route.
- `src/chrome.ts`, `src/layout.ts`: the developer tools' zone, its tabs, side and Focus. `src/search-card.ts`: the
  Pill's address card. `src/viewport.ts`: the app's preview size and frame.
- `src/notifications.ts`, `src/notifications-screen.ts`: the Inbox prototype. `src/chat-tabs.ts`: Chat's Chats | Inbox
  tabs. `src/apps-shelf.ts`: the dock's Apps entry and its shelf.
- `src/workers-menu.ts`: Product → Workers. `src/dev-tools/chat-demo.ts`: More → Chat demo.
- `src/invitations/`: invitations between users' hosts. `src/dev-tools/`: More's dev server controls, test users and
  Example Chat, loaded only by a dev server.
- `src/chat-provider.ts`: the interface between the Chat screen and its chats. `src/chat-inbox.ts`: the session's
  providers. `src/conversation-view.ts`: one conversation, for any provider. `src/nova-chat-provider.ts`,
  `src/product-room-provider.ts`: the Nova and product room providers.
- `src/chat-v2/`: chat products over the Chat Extension v2 draft bridge, and `local-demo.ts`, the page side of the local
  demo, whose dev-server side is `chat-product-demo-plugin.ts`. `contract.ts` holds the contract's types and the bridge
  messages, `host.ts` the host end, `provider.ts` and `calls.ts` the chat provider over it, `product.ts` a product's
  end, and `synthetic-media.ts` the call engine with no devices.
- `src/chat-worker/`: the Nova chat worker. `worker.ts` is its entry, `identity-bridge.ts` the host's lent chat identity
  (over `src/chat/identity.ts`), `truapi-ports.ts` its Statement Store, chain and storage over TrUAPI,
  `conversations.ts` the engine-independent seam and its v2 adapter, `nova-conversations.ts` the Nova engine behind it,
  and `worker-mode.ts` and `engine-switch.ts` the host's switch between the two engines. `src/host-names.ts`: the host's
  names for the people of a chat product's contact rooms.
- `src/worker-manifest.ts`, `src/product-workers.ts`: finding a product's worker and running it. `src/product-chat.ts`:
  the host side of TrUAPI Chat. `src/product-render.ts`: the cards a worker draws.
- `src/archive/`: CAR, UnixFS and CID verification and unpacking.
- `src/sandbox/`, `sandbox-plugin.ts`: the loader, the service worker, the mount paths and the archive cache. The plugin
  bundles them as static files, which the build emits and the dev server also answers.
- `browser/`: the browser smoke test, the inspector dock, Chat view and Chat screen tests, and their fixtures.

## Not supported

- Light-client-verified name resolution. The record's CID comes from one public RPC node.
- Sharded directories in a website record. They are refused with a message, not opened partly.
- Gating of a product's own network, device or media requests. They follow the browser.
- Byte-range requests and non-UTF-8 HTML from a name's archive.
- Versioned hosted releases of this host, which would let a product be tested against a chosen TrUAPI version.
- A Coinage engine, and Chat beyond one device and text. The Pocket screen has no digital dollar balance, and holds only
  the pinned Humanity card. The host passes no payment callbacks, so the core answers those calls `Unsupported`. Chat
  has no phone pairing, history sync, multi-device envelopes, attachments, reactions or calls. Live OS permission status
  is not provided either.
- Several products in front at once. One product is in front, and the others run hidden behind it.
- Keeping held products across a reload or a session change.
- Worker products beyond Chat and the Humanity card. A product opened in the address bar runs as `App`; a worker runs
  from Product → **Workers** for chat, or for Humanity's face, and a `Widget` runs only from the Humanity card.
- Dialling an external wire debugger. `VITE_TRUAPI_DEBUGGER_URL` connects and receives no frames, because the signing
  core's product runtime does not install the debug tap the dial reads. The in-page inspector does not depend on it.
