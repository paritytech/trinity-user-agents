# Running, building and deploying

## Run the dev server

Build the linked packages from the repository root. The host links them from this tree, so it runs the core at this
commit.

```bash
make codegen                                        # also builds @parity/truapi
npm run build --prefix js/packages/truapi-host
npm run build --prefix js/packages/truapi-debugger
make wasm                                           # both truapi-host bundles and truapi-provider
cd hosts/web && npm ci && npm run dev
```

Open <http://localhost:5180>. To try the playground, run `yarn dev` in `playground/` and open `http://localhost:3000`.

| Setting                                 | Effect                                                                     |
| --------------------------------------- | -------------------------------------------------------------------------- |
| `WEB_SIGNING_ALLOWED_HOSTS=a.example,…` | The dev server also answers these host names. By default only `localhost`. |
| `?product=<url>&productId=<id>`         | Fills the address and product id. It does not open the product.            |
| `npm run dev -- --host`                 | Serves on every interface, for a phone on the same network.                |

### Username search route

The identity backend sends no CORS headers. So the dev server proxies its public username search:

`GET /__username-search?network=<id>&prefix=<text>[&limit=<n>][&cursor=<c>]`

- `username-search-plugin.ts` sends only `prefix`, `limit` and `cursor` to `/api/v1/usernames/search` on the network's
  `identityBackend`. It sends no page headers. It passes the answer and errors through.
- It refuses other parameters, other methods and other machines.
- A static build has no such route. It reads usernames on chain instead.

## Build and deploy

`npm run build` writes `dist/`: the host page, `sw.js` (the service worker for mounted products) and `truapi-sandbox/`
(the loader). Any static file server can serve it.

```bash
npm run build -- --base /<repo>/   # GitHub Pages project site: https://<owner>.github.io/<repo>/
npm run build                      # served from the root
npx vite preview --base /<repo>/   # try a build at http://localhost:5180/<repo>/
```

- The base must match the served path. A build for `/repo/` served at `/` loads no assets.
- The build adds `.nojekyll`, so GitHub Pages serves every path.
- A build served from the root ships the Browse seed. The build reads Paseo Next v2 and fails if it cannot. Set
  `WEB_SIGNING_SEED_BROWSE=0` for an offline build. See [The Browse seed](opening-a-product.md#the-browse-seed).
- Contact Chat's worker blocks are also read at the origin's root. A build under another base has neither.

## Secure context and phones

- A product opened by name needs a service worker, so a secure context: `https` or `http://localhost`.
- On `http://<LAN address>:5180`, the host refuses a name before any lookup. A URL product opens. Serve it on its own
  port with the same option, for example `yarn dev -H 0.0.0.0` in `playground/`.
- Camera and microphone also need a secure context. Web Workers, WebAssembly and `getRandomValues` do not, and the host
  needs nothing else on plain HTTP.
- For a phone, the deployed https page needs no setup.
- A browser without service workers in the frame, or without Web Locks, cannot open a name. Embedded web views often
  lack them. Use the system browser.
