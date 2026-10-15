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
  custody.
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

Open <http://localhost:5180>, import a wallet, type a product URL in the address bar and press Enter. To try it with the
playground, run `yarn dev` in `playground/` and open `http://localhost:3000`.

The dev server answers `localhost` only. To reach it by another host name, such as a tunnel, start it with
`WEB_SIGNING_ALLOWED_HOSTS=name.example npm run dev` (a comma list). A static build needs no such setting.

`http://localhost:5180/?product=<url>&productId=<id>` fills in the address bar and product id. The product opens only
when you press Enter or choose **Open**.

## Networks

The menu's **Network** control picks the network this tab runs on. It is chosen before the core starts and applied by
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
levels and paths of 1 KiB. Loading also stops after a deadline. A failed lookup, or an address that cannot be opened, is
shown in the menu and leaves the open product as it is. The product is closed once the lookup succeeds and it is being
opened, so a failure while the archive is fetched, checked or unpacked is shown in the menu and leaves no product open.

The endpoints, TLD and content resolver address are the `paseo-next-v2` values in dotkit's `assets/envs.toml`, and they
match dotli's `packages/config/src/network.ts`.

A name needs a service worker, so it opens only from a secure page: `https`, or `http://localhost`. On another address,
such as `http://<LAN address>:5180` on a phone, it is refused before any lookup, with the reason in the menu. A URL
product opens on any address. The product frame is a secure context only when every page above it is.

A browser that does not offer service workers in the frame, or Web Locks, cannot open a name. An embedded web view often
lacks them: open the host in the system browser.

### The dock

A dock inside the product rectangle opens **Chat**, **Browse** and **Pocket**, as a phone host's tab bar does. Browse is
an ordinary product opened the way the address bar opens it. Chat and Pocket are screens the host draws itself.

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
  destination keeps running. Choosing a destination ends a product opened this way. A name that is already a
  destination's product resumes that destination instead of running a second copy. Any other `http` or `https` address
  goes to the browser, and anything else is refused.
- **The Pocket screen.** The host draws it, laid out as the native Pocket is (the Android `PocketScreen`): a large title
  over two stacked cards, the digital dollar and then Humanity. It reads nothing and signs nothing.
  - **Humanity:** the card the native host pins, drawn from the same bundled face, a title and "Proof of personhood".
    The face is fixed text, not a reading of the wallet's personhood.
  - **Digital dollar:** the native card shows the balance of the wallet's Coinage coins. This host has no Coinage engine
    yet, so the amount slot reads "Unavailable" and says why on hover. No amount is shown, and nothing stands in for
    one.
  - **Not drawn yet:** a card takes no press. The native card opens a details page, which needs the Humanity product or
    the coin balance behind it. Tilt shine, the collectibles strip and removing a card are not here either.
  - The artwork and texture are the native drawables, and the fonts are bundled, so nothing is fetched.
  - `payment.balanceSubscribe` still answers `Unsupported`, so a product sees no balance either. See
    [Not supported](#not-supported).
- **The Chat screen.** See [Chat](#chat).

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
  them. Other message kinds show as a placeholder with their kind number.
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
  copies of it. No captured app traffic and no exchange with a Polkadot app.

### Product chats

A product can chat through its worker executable, as a game product's weekly bot does. The Chat screen lists the rooms
workers keep under **Product chats**, apart from the person-to-person conversations above. These rooms are TrUAPI Chat:
the worker's calls reach the core as frames, which the inspector's **TrUAPI** view decodes.

- **Adding a worker.** **Workers** on the Chat screen takes a product name, such as `myapp.paseo`. The host reads
  `worker.<name>`: its `executable` text record must be a worker manifest (`$v` 1, `kind` `worker`, an `entrypoint`
  module inside the directory) whose `includes.chat` is true, and its `contenthash` locates the files. The archive is
  fetched and verified like a product opened by name, and the entry module is loaded in a hidden frame under the
  product's mount, connected to the core as the product's `Worker` execution. The product id is the name, the one its
  app opens under, so the two share the product's storage and accounts.
- **Lifetime.** The list of workers is kept per network and wallet. A worker runs while its wallet is signed in and the
  tab is open, whichever screen is in front. Signing out, switching wallet or network, or a reload stops every worker
  and ends its frames and render streams; signing in starts the saved ones. **Remove** stops a worker and takes it off
  the list; the rooms it made stay.
- **Rooms and messages.** The host serves TrUAPI Chat (`src/product-chat.ts`): a worker creates rooms, posts `Text` and
  `Custom` messages and sets each room's footer. `registerBot` is refused, as the iOS host refuses it. Other message
  kinds (rich text, actions, files, reactions) are refused with a reason. Rooms and the last 200 messages of each are
  kept per network and wallet in `localStorage` (`truapi-web-signing-host:network:<id>:product-chat:<wallet>:v1`).
- **Custom messages.** A custom message is drawn by asking the worker to render it (`RenderContext::ChatMessage`); each
  tree the worker sends replaces the card. `src/product-render.ts` draws the tree as DOM: text as text, never markup,
  sizes clamped, colors and type from the host's tokens. Images are shown from the worker's verified archive or from the
  network's Bulletin gateway by CID, and from no other address. An archive image is read from the copy in the worker
  frame's own storage: that frame is `credentialless` in Chrome, so the page's storage does not hold it, and the worker
  can write that copy too. A button press and a text field change go back to the worker as renderer actions. A tree
  deeper than 48 or larger than 2000 nodes is not drawn. A text field loses focus when the worker sends a new tree.
- **The room's text input.** A room whose footer is `TextInput` shows a composer. What the user types is kept in the
  room and sent to the worker as a `MessagePosted` chat action from the peer `native`, as the iOS host names the user.
- **Trust.** A worker runs on this host's origin, as a product opened by name does, and is trusted like one: it can
  reach the page and the saved wallets. Its own requests follow the browser. Its notifications, permission requests and
  navigation go through the same callbacks as an app's.
- **Not served yet.** Pocket and input surfaces of a worker, worker demand from the core (a worker runs whenever its
  wallet is signed in), bot registration, and the core's `beginOperation` lifetime hints.

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

## Menu and inspector

The product fills the main area. The top bar holds the brand mark, an editable address bar, the size menu (wide screens
only), and account, inspector and menu buttons. Wallets, the product id and the log live in the menu, which opens from
the right, and the wire inspector sits under the product.

**Address bar.** Typing only edits the text; the bar shows a dashed outline and a go button while it holds an address
that is not open. The product loads on Enter, the go button, or **Open** in the menu, and `Escape` restores the address
that is open. The chip inside the bar shows the product id the open page runs under.

**Product id.** The menu's **Product id** field sets the id the core takes the page for. Leave it empty to derive one
from the address: `localhost:<port>` for a local server, the name itself for a product name, or the host name for a
named site. A bare IP address, such as a phone-reachable LAN URL, derives an id the core refuses, so enter one there. A
line under the field says which id the next Open will use and whether it was entered or derived. The override stays when
the address changes, until **Reset** clears it. Nothing reloads on edit or Reset; the id applies on Enter, the go button
or **Open**.

The id picks which product's accounts, grants, storage and privileges the page gets, including any privilege the core
gives that product, such as signing without a prompt. It is your choice and not a check that the page owns that id.
Enter only ids of products you are developing.

**Menu.** Account, Product and Versions show labels and values. Import a wallet, product Details, Developer, Technical
versions and Log are collapsed. Details holds what was read for a name: record, CID, origin, and what is and is not
checked.

**Layout.**

- **Wide screens (over 900 px).** The menu is a right-hand sidebar and the inspector a bottom dock, open independently.
  Both resize the product frame.
- **Narrow screens.** The menu is a drawer and the inspector a bottom sheet, each over the product with a scrim. One is
  open at a time, the product and top bar behind it are inert, `Escape` or the scrim closes it, and focus goes back to
  the button that opened it. **Open** closes the drawer.

Toggling a panel changes classes and sizes around the product frame. It does not move or re-create the frame, so the
product is not reloaded and the session is untouched.

**Simulating another screen.** The size menu (Fill, Phone, Tablet, Desktop) resizes only the product rectangle. It is a
plain rectangle with no device frame, clock or status bar. The product lays out at exactly the chosen CSS size (393 ×
852 for Phone) and the rectangle is shrunk to fit. The host's own menu and inspector stay at desktop size. The size is
kept for the tab. On a narrow screen the control is hidden and the product fills the screen.

**Inspector.** It is `createInAppDebugger` from `@parity/truapi-debugger`. The page taps the frames it relays between
the product and the core, in both directions, and the debugger groups and decodes them. Decoding is gated on the core's
own wire schema: if it differs from the page bundle's, the inspector lists operations without decoding them and the log
says so. Frames stay in the tab, may carry sensitive payloads, and are held in memory only.

**Chat diagnostics.** The inspector's **Chat** tab lists what the host's chat engine does: start, stop, reconnects,
subscriptions, requests received and why one was set aside, key lookups and which chain named the key, session requests
with the kind number of each message, responses, and every submission with its purpose and the store's answer. Chat is
not a product, so none of this is a TrUAPI frame, and it is listed apart from the traces. The events are metadata only:
shortened accounts and ids, kinds, counts and error text with hex shortened. They never hold keys, shared secrets,
message text, push tokens or decrypted payloads. The last 300 are kept in memory and cleared when the session changes.

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

**Account status** in the menu has two groups. Everything in it is read-only and nothing polls.

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
- opening the menu or inspector, or resizing the product, does not reload it;
- an entered product id is shown as entered;
- the dock keeps Browse's product running while a host screen is in front, resumes it with its page state, and ends the
  held product when the wallet changes;
- Chat does not run or store anything for the public development phrase;
- Chat starts at sign-in while another screen is in front, with no chain connection, derives the same identity account
  and chat key as the core, has no on or off controls, and stops at sign-out;
- Pocket is drawn by the host as the native two cards, and gives the digital dollar no amount;
- a product worker, built from `browser/chat-worker.ts` and served from a stood-in Asset Hub and gateway, runs from its
  manifest, creates a room, posts text and a custom card, draws an image from its archive in the card, redraws the card
  when its button is pressed, answers what the user types, makes no request outside this host, shows in the inspector,
  stops at sign-out, is not seen by another wallet, and starts again when its wallet signs back in.

The smoke test runs against the Vite dev server. It does not show that a static host serves the build: open the built
`dist/` from the host you will use, as described in [Static build](#static-build-and-github-pages).

Not covered by automated checks, and tried by hand in Chrome when the behavior changes: WebRTC, camera and microphone
capture, the phone setup, two tabs with different wallets, the build under a project path, the inspector's rendering,
and layout on narrow screens.

## Layout

- `src/main.ts`: the page and its wiring. `src/product.ts`: embedding a product and relaying its frames to the core.
  `src/callbacks.ts`, `src/prompt.ts`, `src/reviews.ts`: the host callbacks and prompts.
- `src/wallets.ts`, `src/storage.ts`: wallet and per-wallet, per-network storage. `src/network.ts`,
  `src/network-config.ts`, `src/network-choice.ts`: the light client, the networks it serves and the per-tab choice.
- `src/dotns.ts`, `src/address.ts`: name lookup and the address bar's grammar.
- `src/shell.ts`: the dock's destinations and saved addresses, and what a product's `navigateTo` turns into.
  `src/pocket.ts`, `src/pocket-screen.ts`: the Pocket cards and how the screen draws them.
- `src/chat/`: the chat engine. `gate.ts` refuses the public development phrase, `keys.ts` derives the keys,
  `protocol.ts` and `statement.ts` are the wire, `statement-store.ts` the store's JSON-RPC, `directory.ts` the chain
  reads, `store.ts` the saved conversations and `engine.ts` ties them together, and `diagnostics.ts` holds its events.
  `src/chat-screen.ts` draws the Chat screen and `src/chat-diagnostics-view.ts` the inspector's Chat tab.
- `src/worker-manifest.ts`, `src/product-workers.ts`: finding a product's worker and running it. `src/product-chat.ts`:
  the host side of TrUAPI Chat. `src/product-room-view.ts`, `src/product-render.ts`: a product room and the cards its
  worker draws.
- `src/archive/`: CAR, UnixFS and CID verification and unpacking.
- `src/sandbox/`, `sandbox-plugin.ts`: the loader, the service worker, the mount paths and the archive cache. The plugin
  bundles them as static files, which the build emits and the dev server also answers.
- `browser/`: the browser smoke test and its fixtures.

## Not supported

- Light-client-verified name resolution. The record's CID comes from one public RPC node.
- Sharded directories in a website record. They are refused with a message, not opened partly.
- Gating of a product's own network, device or media requests. They follow the browser.
- Byte-range requests and non-UTF-8 HTML from a name's archive.
- Versioned hosted releases of this host, which would let a product be tested against a chosen TrUAPI version.
- Contacts, a Coinage engine, and Chat beyond one device and text. The Pocket screen has no digital dollar balance and
  opens no card. The host passes no `contacts`, `pocket` or payment callbacks, so the core answers those calls
  `Unsupported`. Chat has no phone pairing, history sync, multi-device envelopes, attachments, reactions or calls. A
  contact picker needs `ContactsPlatform` (`contacts` and `pickContact`) and a list of contacts the host owns. Live OS
  permission status is not provided either.
- Several products in front at once. One product is in front, and the others run hidden behind it.
- Keeping held products across a reload or a session change.
- Worker products beyond chat. A product opened in the address bar runs as `App`; a worker runs only from the Chat
  screen's **Workers**, and only for chat.
- Preimage lookup. Every lookup is a miss. The core submits preimages to Bulletin itself.
- Dialling an external wire debugger. `VITE_TRUAPI_DEBUGGER_URL` connects and receives no frames, because the signing
  core's product runtime does not install the debug tap the dial reads. The in-page inspector does not depend on it.
