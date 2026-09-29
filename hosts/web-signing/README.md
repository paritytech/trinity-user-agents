# Web signing host

A TrUAPI host that runs in a browser tab and holds its own wallet. It embeds a product in an iframe, runs the core from
this repository in a Web Worker, and signs locally. There is no companion app and no QR pairing.

This is a development host. The dotli host at `hosts/dotli/` is the production web host: it pairs with the Polkadot app,
which holds the keys, and the production browser core (`@parity/truapi-host/wasm/web`) has no signing host in it for
that reason. This host loads the `testing` core instead, the one bundle built with `wasm-signing-host`, and keeps the
recovery phrases it signs with in the browser.

## Security boundary

Recovery phrases are saved **unencrypted** in `localStorage` on the host's origin. Any script on that origin can read
them. Use wallets you are willing to lose.

A product always runs on its own origin, which is what keeps it away from those phrases. The host refuses to open a
product served from its own origin. Nothing else stands between a product and the wallet: the review prompts are the
consent UI of a development tool, not custody.

## Run it

The host links `@parity/truapi`, `@parity/truapi-host` and `@parity/truapi-provider` from this tree, so it runs against
the core at this commit. Build them first, from the repository root:

```bash
make codegen
npm run build --prefix js/packages/truapi-host
make wasm
```

`make codegen` also builds `@parity/truapi`. `make wasm` builds both `truapi-host` WASM bundles and the
`truapi-provider` bundle. Then:

```bash
cd hosts/web-signing
npm ci
npm run dev
```

Open <http://localhost:5180>, sign in, enter a product URL and choose **Open**. To try it with the playground, run
`yarn dev` in `playground/` and open `http://localhost:3000`.

`http://localhost:5180/?product=<url>&productId=<id>` fills in the product form. The product opens only when you choose
**Open**.

## Wallets and sessions

- **Built-in dev accounts.** `alice`, `bob`, `charlie` and `dave` are the test host's dev accounts from
  `@parity/truapi-host/testing/dev-accounts`. Their entropy is public.
- **Saved wallets.** **Create or import** with an empty phrase creates a 12-word wallet and shows its phrase once. With
  a phrase it imports that wallet. One phrase is one wallet, however often it is imported.

A session belongs to the tab. Each tab runs its own core, so two tabs can be signed in with different wallets at the
same time. A reload restores the tab's wallet from `sessionStorage`.

Product and core storage live in `localStorage` under the active wallet's id, so one wallet never reads or overwrites
another wallet's grants, AutoSigning keys or product data. The device encryption key is the exception: the core requires
it to outlive logout and per-user namespacing, so it is stored once per browser profile.

Switching wallets closes the product and opens it again under the new session, because a product runtime must not
outlive the session it was opened under.

## Products

A product is an `http` or `https` URL. Its product id is the URL's host: `localhost:<port>` for a local server, which
the core accepts as a development id. Any other URL needs a dotNS product id in the **Product id** field, since the core
refuses ids that are neither.

The product id decides what the core trusts the page as. An id you enter gives the page that product's accounts, grants
and storage, and any privilege the core gives that product, such as signing without a prompt. Enter only ids of products
you are developing.

## Chain access

Chains are reached through the embedded light client in `@parity/truapi-provider`, using its bundled `paseo-next-v2`
catalog. The genesis hashes the core is configured with and the chain set reported to products come from the same
catalog. The light client keeps no state between loads, so each tab syncs from the chain spec checkpoint when it starts.

## Not supported

- dotNS names. A product is opened by URL only, and a `polkadot://` navigation is logged and not followed. Serving a
  product archive from this host's origin would give the product access to the saved phrases.
- Chat, contacts, Pocket and live OS permission status. The core answers those calls `Unsupported`.
- Worker products. Products run as `App`.
- Preimage lookup. Every lookup is a miss. The core submits preimages to Bulletin itself.
- The wire debugger. The signing core does not install the debug tap, so a dial set with `VITE_TRUAPI_DEBUGGER_URL`
  connects and receives no frames.
