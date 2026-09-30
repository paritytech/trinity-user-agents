# Web signing host

A TrUAPI host that runs in a browser tab and holds its own wallet. It embeds a product in an iframe, runs the core from
this repository in a Web Worker, and signs locally. There is no companion app and no QR pairing.

It is a tool for developers, run on a development machine or a private network, to try a product against the current
core. It is not a production host. The dotli host at `hosts/dotli/` is the production web host: it pairs with the
Polkadot app, which holds the keys, and the production browser core (`@parity/truapi-host/wasm/web`) has no signing host
in it for that reason. This host loads the `testing` core, the bundle built with `wasm-signing-host`, and keeps the
recovery phrases it signs with in the browser.

## Trust model

This tool trusts its user and the products the user chooses to open. It is not built to contain a hostile product.

- **Wallets are disposable.** Recovery phrases are saved unencrypted in `localStorage` on the host's origin. Any script
  on that origin can read them. Import a phrase you can lose, never one that holds value.
- **A product runs on its own origin.** The host refuses to open a product served from its own origin, and the server
  does not serve the sandbox loader or its scripts on the host's origin. That keeps a product's own storage apart from
  the wallets. The host has no setting that turns the separation off.
- **The permission prompts are a development aid.** They show what a product asks for and let you answer. They are not
  custody.
- **The container is a gate for cooperating pages.** A page that loads the shared container asks the core before it uses
  the network or a device. This works for the code a developer writes and the libraries it uses. It does not stop a page
  that means to get around it: a page can create a browser context the container has not patched, such as an
  `<iframe srcdoc>`, and use the built-ins there. Do not open a product you would not open in a browser tab.
- **Anyone who can reach a served host can use it.** Serve it on loopback, or on a private network, and use a disposable
  wallet.

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
cd hosts/web-signing
npm ci
npm run dev
```

Open <http://localhost:5180>, import a wallet, type a product URL in the address bar and press Enter. To try it with the
playground, run `yarn dev` in `playground/` and open `http://localhost:3000`.

`http://localhost:5180/?product=<url>&productId=<id>` fills in the address bar and product id. The product opens only
when you press Enter or choose **Open**.

## Opening a product

The address bar takes a web address or a product name. What the container can do for the page depends on how it was
opened.

| Opened as                               | Where the page runs                                             | Container                                                                                                      |
| --------------------------------------- | --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| A name, such as `myapp.paseo`           | An origin of its own, served by this host from a service worker | Added to every HTML page the host serves, before the page's own scripts. The default.                          |
| A URL, with **Expect container** ticked | The URL's own origin                                            | The page loads it and announces itself. The host trusts the page and cannot check.                             |
| A URL, plain                            | The URL's own origin                                            | None. The status line says `Container: none`. The page uses the network, storage and built-ins without asking. |

**Web addresses.** An address without a scheme gets `http://`. Only `http` and `https` are accepted. `localhost`, LAN
addresses and ordinary sites keep their meaning.

**Product names.** `myapp.paseo` opens the product published under that DotNS name. `http://myapp.paseo/` and
`polkadot://myapp.paseo` mean the same name, and a path, query and hash after the name are kept. A name is one label and
the TLD of the connected network, `paseo`. The core recognises `dot`, `paseo` and `testnet` as DotNS TLDs, so `.dot` and
`.testnet` names are refused with a message saying they belong to another network, and so are names with a port or
credentials or with more than one label. `https://myapp.paseo.li` is refused too: that public gateway serves the
Polkadot browser shell for every name and refuses to be embedded, so the message says to type the name.

### How a name opens

On Open the name is looked up on Paseo Asset Hub, its content is fetched from the Bulletin IPFS gateway and checked, and
it is served from an origin of its own. The product id defaults to the name.

1. The host namehashes a record name and reads the DotNS content resolver's `contenthash` from Asset Hub over
   `wss://paseo-asset-hub-next-rpc.polkadot.io`. It tries `app.<name>` first, then `<name>`. `app.<name>` is where
   `dotkit deploy` publishes the product's app executable, and the Polkadot browser reads it first too.
2. It fetches the first bytes of each CID from the gateway to see what it is. An app executable is a CARv1 archive of
   the site's directory, stored as one chunked UnixFS file. A website is a UnixFS directory. Both open. A record that is
   neither is skipped and listed.
3. It opens a loader page on `<label>.localhost:<port>`, where the label is the product id with a hash of it, so two ids
   never share an origin. The loader registers a service worker, fetches the blocks with `?format=raw`, checks each one
   against its CID, unpacks the site in memory, stores the files in Cache Storage on that origin, and then navigates to
   the start path. The worker serves the files and adds the container to HTML pages.

The loader takes its instructions from the address of the frame that embeds it, so it checks who embedded it before it
touches storage or a worker. It runs only inside a frame of this host: at top level, in a frame of any other page, or
with a host or owner that does not match, it refuses and fetches nothing. The server also sends a `frame-ancestors`
header for it.

Integrity, in two steps that should not be confused:

- **Bytes.** For an app archive, every chunk of the archive file is checked against the CID the record names, and every
  block inside the archive against its own CID. For a website, every block from the record's CID down. A gateway that
  sends anything else is refused, so the gateway is not trusted. An archive's inner root is not on chain, so the second
  check proves only that the site is consistent. The first is what ties it to the name.
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

The origin needs no setup on a loopback host, because `*.localhost` labels are separate secure origins. A host reached
by IP address or another name cannot use it: a service worker needs a secure context, and no wildcard name is known to
resolve. Open the host at `http://localhost:5180`, or start the server with `WEB_SIGNING_SANDBOX_ORIGIN` set to a
template such as `https://{label}.sandbox.example` that serves this host's build with a valid certificate. A name then
fails with that explanation, and a URL product still opens. See [Operator settings](#operator-settings).

### The container and its private channel

The iOS and Android hosts inject `js/container` before any product code. It replaces `fetch`, `XMLHttpRequest`,
`WebSocket` and WebRTC with versions that ask the host first, removes `indexedDB`, `caches`, `EventSource`, `Worker`,
service workers and the Cookie Store API, makes `document.cookie` a no-op, and freezes the built-ins the permission path
depends on. A web host cannot script a page on another origin before that page's own code runs, so the container reaches
a page here in the two ways in the table above.

How the channel between the container and the core works:

- The container waits for a private `MessagePort` from the host, and holds every gated request until it has one. A
  request made in the page's first script is held, prompts, and is sent only if allowed.
- The container takes the port with a capture-phase listener that stops propagation, from its parent window only, and
  only the first. In an archive the container is told the host's origin and accepts the port only from it.
- The host splits the one core connection by request id. Frames with the container's `host:` prefix go only to the
  private port, and the same prefix arriving on the product's port is dropped, so a product does not read the
  container's replies or send as the container.
- Without **Expect container**, or for a URL page that did not load it, a container announcement is ignored and no
  private port is given.
- If the private port dies mid-session, gated requests wait and then fail after the 120 s request timeout, and nothing
  is sent. A permission dialog raised just before the loss stays open.
- Served HTML gets the page policy
  `frame-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; worker-src 'none'`. It narrows what markup
  can load. It is a convenience for well-behaved pages, and it does not make an archive a sandbox: see the trust model.

### Differences from the native hosts

These are accepted for a development host.

- Android authorizes every HTTP request natively, including images, scripts and stylesheets. A browser cannot intercept
  those. The container's gates cover `fetch`, XHR, WebSocket, WebRTC and media, on iOS and here.
- A URL product is cooperative: the host trusts a ticked page to have loaded the container. The menu says so until the
  container connects.
- Network, media and WebRTC prompts appear only for what the container gates. Secure-context APIs are unavailable on
  plain HTTP, and native-only APIs are not emulated.
- The frame gets `allow="camera; microphone"` only when a container is expected. Delegation only lets a request reach
  the browser: the container still asks the core first, and the browser's and the operating system's prompts still
  apply. Camera and microphone need a secure context, so on `http://<LAN address>` a product's capture call throws
  before any prompt.
- Storage on `.localhost` origins is separate per product. Cookies ignore the port, so on a shared name (see
  [Private HTTPS](#private-https)) the container cannot be turned off.

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
says so. Frames stay in the tab, may carry sensitive payloads, and are held in memory only. **Clear** drops them.

**Versions.** The menu shows client, host and core, provider and debugger versions, read from the linked packages and
the loaded `testing` bundle. A warning appears only when the core's wire schema differs from the client's. **Technical
details** holds the SHA-256 of the served `.wasm` files, the wire schema, and the source commit with whether the tree
has uncommitted changes. Facts that cannot be read show as `unknown`. Build-time values are measured when the dev server
starts or the build runs, so restart the dev server after rebuilding a WASM bundle.

A tab that opens a product opens it again after a reload, once its wallet is restored. A `?product=` link only fills the
form.

## Operator settings

Three environment variables are read when the dev or preview server starts, or when `npm run build` runs. Restart the
server after changing one. The sandbox settings are compiled into the loader and the service worker, so neither a link
nor the menu can change them.

- `WEB_SIGNING_SANDBOX_ORIGIN`: where product origins live. Empty means automatic `<label>.localhost` beside a loopback
  host. Otherwise `https://{label}.sandbox.example`, or a port range on one name such as
  `https://host.example:{9450-9459}`.
- `WEB_SIGNING_HOST_ORIGINS`: a comma list of the origins that may embed the loader, which are the host's own origins.
  It is derived when empty: `localhost` and `127.0.0.1` on the product's scheme and port in loopback mode, and the
  template's host on its default port for a port range, which fits the Tailscale setup. A `{label}` template, or a port
  range with the host on another port, needs it set. Without a trusted host origin the loader is refused.
- `WEB_SIGNING_ALLOWED_HOSTS`: host names, besides `localhost`, the server answers to (Vite refuses other `Host`
  headers).

The dev and preview servers answer requests for the loader and its scripts only on product origins, and serve the loader
only into a frame. That filter is part of the Vite plugin. A different static host serving `dist/` applies no such
filter, and files under `/__sandbox/` are then served on every origin it answers to. The loader and the service worker
carry their own checks, compiled from the operator settings: the loader must be on a product origin and inside a frame
of a trusted host, and the link's `host` and `owner` must match, before it touches storage or a worker. Use the Vite
servers, or add the same filtering, when the host's origin also serves the build. An IPv6 host such as `http://[::1]` is
not supported, since a `frame-ancestors` list cannot name it. Use `localhost` or `127.0.0.1`. Claims on an origin and on
a port use Web Locks, which need a secure context.

A service worker that an earlier build left on the host's own origin retires itself at its next update check and reloads
its pages. Nothing is cleaned up beyond that: caches named `truapi-archive` and `truapi-sandbox-owner` on the host's
origin stay until you clear them in DevTools (Application, Storage).

## Developer settings

The menu's Developer section shows the sandbox origin, read-only (see [Operator settings](#operator-settings)), and
holds two relaxations, each on purpose and per tab:

- **Run archives without the container.** Only where the origin is a `.localhost` label of its own. The worker serves
  HTML as it is, so the product's requests are not gated. It stays on its own origin.
- **Approve network requests without asking.** Each network permission prompt is answered with a one-time yes, and the
  log records each one.

Both start off. A banner under the top bar stays while any relaxation is on, in force or chosen for the next Open.
Changing one applies when the product is opened again, and **Turn off and reopen** on the banner does both. The choice
is kept in the tab's `sessionStorage`, so a reload keeps it. Separation from the wallets is not on the list.

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
  committed People chain metadata, so a runtime that changed them shows "Unreadable". **Consumer slots** is the
  `stmt_store_slots` field of the record as stored. It is not a product allowance and not remaining quota.
  **Ring** rows show where `Members.Members` holds the ring key that the account's own `PeopleLite.LitePeople` record
  (lite), or `People.AccountToPersonalId` and `People.People` records (full), name: Onboarding, Included with its ring
  index, or Suspended, read on the same finalized block. "No Members entry" means a key is named and unlisted. "No ring
  key on record" means no record names one, which is not a finding that the person is outside the rings. The key is the
  chain's, not derived here, so the rows are a diagnostic and do not by themselves explain a signing failure.
- **Current product** shows the open product's resources. **Allocation recorded** is the core's local renewal ledger: it
  lists a product only after an allocation reached the chain, so it is history, not a check of the chain. The rows
  below it are read by the core on Refresh, at the finalized block of each chain, from public accounts it derives for
  the product. Nothing is allocated, renewed, claimed, registered or signed to read them.
  - **Statement Store** reads `Resources.StmtStoreAllowanceByAccount` on the People chain for the product's allowance
    account (`//allowance//statement-store//{productId}`). An entry for the chain's current period reads "Allocated ·
    this period" with its slot. An earlier period reads "In grace window" or "No current allocation", by the runtime's
    grace window. An empty index reads "No allocation". It is not remaining quota.
  - **Bulletin** reads `TransactionStorage.Authorizations` for the Bulletin allowance account: remaining transactions
    and bytes (allowance minus used) and the expiry block, or "No authorization" or "Expired".
  - **PGAS** reads the balance of the product's account 0 on Asset Hub, using the asset id and claim amount from the
    runtime's own metadata and the decimals and symbol from `Assets.Metadata`. It is a fee balance, not a permission,
    and only account 0 is read; a product may use others.

  A part that could not be read shows "Unavailable" with the reason and leaves the other parts intact. Needs a core
  built with `productResourceStatus` (the `testing` bundle built from this tree): an older bundle shows "Needs a newer
  core". A reply is dropped when the wallet changes before it arrives, and a read for another product is not shown.

A failed read changes nothing about the session.

A session belongs to the tab. Each tab runs its own core, so two tabs can be signed in with different wallets at the
same time. A reload restores the tab's wallet from `sessionStorage`.

Product and core storage live in `localStorage` under the active wallet's id, so one wallet never reads or overwrites
another wallet's grants, AutoSigning keys or product data. The device encryption key is the exception: the core requires
it to outlive logout and per-user namespacing, so it is stored once per browser profile.

Switching wallets closes the product and opens it again under the new session, because a product runtime must not
outlive the session it was opened under.

## Opening it from a phone

`npm run dev -- --host` serves the host on every interface. Open `http://<this machine's LAN address>:5180` on a phone
on the same network. Serve the product on its own port with the same option, for example `yarn dev -H 0.0.0.0` in
`playground/`, and give its LAN URL as the product URL: the host refuses a product on its own origin, and a phone cannot
reach `localhost`.

**Products opened by name need HTTPS or localhost.** A name opens only from a page that is a secure context, with an
origin of its own per product:

- **localhost or 127.0.0.1, over http.** Works with no setup. Each product gets `<label>.localhost:<port>`.
- **HTTPS host.** Works when the server was started with an https `WEB_SIGNING_SANDBOX_ORIGIN`:
  `https://{label}.sandbox.example` for a wildcard name with a certificate that covers every label, or a port range on
  one name, `https://host.example:{9450-9459}`. See [Private HTTPS](#private-https).
- **Anything else, such as `http://<LAN address>:5180` on a phone.** A name is refused before any lookup, with the
  reason in the menu and under the address bar. A frame is a secure context only when every page above it is.

There is no fallback: a name is never opened by navigating the frame to the gateway, or from a shared origin, because
that would drop the container and the separation from the wallets. URL products open on any address.

Plain http is enough for everything else. Web Workers, WebAssembly and `getRandomValues` work outside a secure context,
and the host uses nothing that does not. The chain connections are `wss://` from the embedded light client.

## Private HTTPS

This serves the host and its products over https to devices on your tailnet, such as a phone, with a real certificate.
It uses Tailscale Serve, so it needs Tailscale on this machine with HTTPS certificates enabled for the tailnet (admin
console, DNS), and Tailscale on the phone, signed in to the same tailnet.

One name has one certificate and its subdomains do not resolve, so products are told apart by port: the host is on 443
and each product owns one port of a range. A port is a separate origin, so its `localStorage`, `sessionStorage`,
IndexedDB, Cache Storage and service worker are separate. Cookies are not: they are keyed by host name and ignore the
port.

1. Build with the operator settings and serve the build on loopback. The node name is the `DNSName` in
   `tailscale status --json`, without the trailing dot.

   ```bash
   NAME=<node>.<tailnet>.ts.net
   WEB_SIGNING_ALLOWED_HOSTS=$NAME WEB_SIGNING_SANDBOX_ORIGIN="https://$NAME:{9450-9459}" npm run build
   WEB_SIGNING_ALLOWED_HOSTS=$NAME WEB_SIGNING_SANDBOX_ORIGIN="https://$NAME:{9450-9459}" \
     npx vite preview --host 127.0.0.1 --port 5181
   ```

2. Route it with `scripts/tailscale-serve.sh up`. Set `BACKEND` (default `http://127.0.0.1:5181`) and `PORTS` (default
   `9450-9459`, ascending, without 443) to match step 1. The script prints the host, product and sandbox origin URLs.
3. Open `https://<NAME>/` on the phone, import a disposable wallet, and type a name such as `chat-spa-probe.paseo`.

What the script does, in `scripts/tailscale-serve.sh`:

- `up` records each route it makes in a state file, by port with its host name and backend, together with the Serve
  config it found before the first `up`. The state file is `$STATE`, by default
  `${XDG_STATE_HOME:-~/.local/state}/truapi-web-signing-host/tailscale-serve.json`. It holds your tailnet name, so keep
  it out of git. The saved config is for you to read; nothing replays it.
- `up` is safe to repeat. It refuses, and changes nothing, when one of its ports is already used by something else or
  has Funnel on. A route that already points at `BACKEND` and was not made by the script is left alone, and `down`
  leaves it too. If creating a route fails, it undoes the routes it made in that run. It never enables Funnel.
- `down` removes only routes recorded in the state file that still are exactly what `up` made, on the same host name,
  port and backend, and reports the rest. A route whose live host name changed is left alone. It ignores `BACKEND` and
  `PORTS`. Routes made by an earlier version of the script are not in the new record and are not removed by `down`.
  `status` shows what is routed and what the script made.
- A lock directory, `$STATE.lock`, stops two runs of the script at once. If a run crashed and left it, delete it. It
  does not guard against other tools changing Serve at the same time.
- A state file that is not a valid record, or is an old one, is never overwritten or removed: `up` and `down` stop and
  say to inspect `tailscale serve status` and remove the old state file if appropriate.
- The script is a convenience for one developer's machine. It does not protect the state file from other users on the
  machine, and it does not restore the saved config.

The range holds 10 products by default. A product keeps the port it was first given, in this browser, and a port is
never given to a second product: a full range is an error, so widen `PORTS` and the build setting together. The sandbox
origin is preset by the build, never by a link, because a link that could set it could change where a checked archive is
served from.

To undo it, run `scripts/tailscale-serve.sh down`, then stop the preview.

**Cookies on a shared name.** Ports of one name share one cookie jar, so on this setup the container is required and
cannot be turned off. Archives without the container are refused wherever products share a name: the menu's checkbox is
disabled, a saved choice is dropped, and the service worker ignores a `container=off` request on any origin that is not
a `.localhost` label. With the container on, `document.cookie` is a no-op and the Cookie Store API is removed. A product
can still ask the browser to send existing cookies to this name, for example by fetching another port, if you allow that
request at the prompt, and a page that makes an unpatched realm (see the trust model) can reach the cookie jar directly.
The wallet phrases are in `localStorage` of the host's origin, and no cookie holds them.

**Ports the ledger forgot.** The port ledger lives in the host page's `localStorage`. If it is lost, a port may be
handed out again while its origin still holds another product's retained data. So the loader records the product on the
origin at first use, and refuses the origin to any other product. The host then retires that port and skips it: nothing
is wiped, reassigned or shown to the other product, and Open again gives a fresh port.

Other limits:

- Anyone on the tailnet can reach the host, not only your own devices. Use a disposable wallet.
- Chrome on desktop is what is tested. Other browsers are unverified, and Safari and Firefox have not been tried. The
  loader needs `location.ancestorOrigins` to identify the page that embeds it. In a browser without it, the loader
  refuses to start and says the browser is unsupported. Where the `credentialless` frame attribute is missing, product
  storage may persist between loads.
- The state file and `dist/` contain your tailnet name and stay out of git.

## Chain access

Chains are reached through the embedded light client in `@parity/truapi-provider`, using its bundled `paseo-next-v2`
catalog. The genesis hashes the core is configured with and the chain set reported to products come from the same
catalog. The light client keeps no state between loads, so each tab syncs from the chain spec checkpoint when it starts.
A name lookup uses one public RPC node instead, as described above.

## Tests

From `hosts/web-signing`:

```bash
npm run typecheck   # tsc --noEmit
npm test            # unit tests: src/ and scripts/ (bun test)
npm run build       # tsc --noEmit && vite build
npm run test:browser
npm run test:sandbox
```

CI runs all of these in the `host-web-signing` job of `.github/workflows/ci.yml`, after building the packages this host
links, so it runs against the core at the same commit. These are best-effort gates for a development tool: they catch a
broken host, not a hostile product.

`npm run test:browser` is a headless-Chrome smoke test of the host page. It needs the linked packages and both WASM
bundles built (see [Run it](#run-it)) and a Chromium: install one with `npx playwright-core install chromium`, or set
`CHROMIUM_PATH`. It starts the host with Vite on loopback, a counting target server and product fixtures, stands in for
Asset Hub and the Bulletin gateway inside the browser context, and signs in with a public test phrase. It touches no
live chain, no tailnet and no real wallet. It checks that:

- a name opens from an archive on its own origin, which cannot see the host's storage, and the host's origin serves no
  loader or scripts;
- with the container, the page's first request waits for the prompt, a denial sends nothing to the target, an allow-once
  sends one request, and always-allow stops the prompts, counted at the target;
- a URL product with **Expect container** ticked behaves the same, and the host refuses its own origin;
- opening the menu or inspector, or resizing the product, does not reload it or drop the private channel;
- the Developer settings start off, apply on the next Open, survive a reload, and turn off again, and an entered product
  id is shown as entered.

`npm run test:sandbox` runs the sandbox boundary check in `src/sandbox/e2e/boundary.e2e.mjs` in the same Chromium: the
loader against a wallet origin, a top-level visit, a parent that is not the host, a wrong owner or host, a stale worker,
and concurrent port allocation.

Not covered by automated checks, and tried by hand in Chrome when the behavior changes: WebRTC, camera and microphone
capture, the 120 s private-port timeout, the phone and Tailscale setup, the inspector's rendering, and layout on narrow
screens.

## Layout

- `src/main.ts`: the page and its wiring. `src/product.ts`, `src/container-channel.ts`: embedding a product and the
  private channel. `src/callbacks.ts`, `src/prompt.ts`, `src/reviews.ts`: the host callbacks and prompts.
- `src/wallets.ts`, `src/storage.ts`: wallet and per-wallet storage. `src/network.ts`: the light client.
- `src/dotns.ts`, `src/address.ts`: name lookup and the address bar's grammar.
- `src/archive/`: CAR, UnixFS and CID verification and unpacking.
- `src/sandbox/`, `sandbox-plugin.ts`: the loader, the service worker and the server's rules for product origins.
- `browser/`: the browser smoke test and its fixtures. `scripts/`: `tailscale-serve.sh`, its Python helper, and its
  tests (`scripts/tailscale-serve.test.ts`, run against a fake `tailscale` in `scripts/testing/`).

## Not supported

- Light-client-verified name resolution. The record's CID comes from one public RPC node.
- Sharded directories in a website record. They are refused with a message, not opened partly.
- Gating of subresources written in a product's markup, such as `<img>`, `<script src>` and stylesheets.
- Byte-range requests and non-UTF-8 HTML from a name's archive.
- Versioned hosted releases of this host, which would let a product be tested against a chosen TrUAPI version.
- Navigation started by a product. A product that asks to go to a `polkadot://` name is logged and not followed.
- A product that loads a new document in its frame (a link, a form post, `location.assign`). Single-page navigation is
  fine. When a page that loads the container does this, the host closes its channels, shows "Container: ended" and turns
  Open into **Reopen**. A page opened without the container is not watched and simply loses its channel.
- Chat, contacts, Pocket and live OS permission status. The core answers those calls `Unsupported`.
- Worker products. Products run as `App`.
- Preimage lookup. Every lookup is a miss. The core submits preimages to Bulletin itself.
- Dialling an external wire debugger. `VITE_TRUAPI_DEBUGGER_URL` connects and receives no frames, because the signing
  core's product runtime does not install the debug tap the dial reads. The in-page inspector does not depend on it.
