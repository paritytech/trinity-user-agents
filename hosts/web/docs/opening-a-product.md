# Opening a product

| Opened as                     | Where the page runs                                           |
| ----------------------------- | ------------------------------------------------------------- |
| A name, such as `myapp.paseo` | A mount under the host's own path, served by a service worker |
| A URL                         | The URL's own origin                                          |

- Both reach the core through `createIframeHost`. The frame keeps its default `sandbox`
  (`allow-forms allow-same-origin allow-scripts`): no popups, no `alert`, no downloads.
- The host adds `allow="camera; microphone"`. This lets the browser ask. It grants nothing.

## Address rules

- An address without a scheme gets `http://`. Only `http` and `https` open.
- A name is one label plus the network's TLD. `http://myapp.paseo/` and `polkadot://myapp.paseo` are the same name. A
  path, query and hash stay.
- The host refuses:
  - a name with another network's TLD (`dot`, `paseo` and `testnet` are DotNS TLDs);
  - a name with a port, credentials or more than one label;
  - the network's public gateway, such as `https://myapp.paseo.li`. It serves the browser shell and refuses framing.
- A bare label such as `myapp` gets the network's TLD when it opens. The field shows the TLD dimmed. A dot, port, scheme
  or brackets mark the text as complete, so `localhost:3000` and `devbox:3000` stay as typed. A single-label LAN host
  without a port needs `http://`.
- **Recents.** The field lists the products that this wallet opened. Each entry keeps its product id. History holds
  addresses and ids only. It skips addresses with credentials or secret-looking parameters (`token`, `key`, `code`,
  `session` and similar).
- **Reset app data** (Product tab) clears the core's data for the open product and wallet, after a confirmation, and
  reopens it. It does not touch grants, allowances, signing state, other products, other wallets, browser storage or
  other tabs. Close other tabs with the same wallet and product first.

## How a name opens

1. The host reads the DotNS `contenthash` of `app.<name>`, then `<name>`, from the Asset Hub RPC node. `dotkit deploy`
   publishes the app executable at `app.<name>`.
2. It reads the first bytes of the CID from the gateway. An app executable is a CARv1 archive in one chunked UnixFS
   file. A website is a UnixFS directory. The host skips and lists anything else.
3. The loader (`<base>truapi-sandbox/index.html`) registers a service worker for the mount
   `<base>product/<wallet>/<product>/<content>/`, fetches and checks the blocks, unpacks the site, and stores the files
   in Cache Storage under the content id.
4. The frame moves to the start path. The service worker serves the files with their type and no sniffing.

- The product id defaults to the name.
- The mount path holds a hash of the wallet id, the product id (readable, with its hash) and the content id. A different
  wallet, product or content gets a different mount and service worker.

### Block fetching

1. The loader asks for one CAR stream of the whole DAG: `?format=car`, `Accept: application/vnd.ipld.car`.
2. Reads wait up to 2 s for the stream. This 2-second wait before raw block requests is the **hedge** (the Products
   view's name). After it, a missing block is fetched alone with `?format=raw`: at most 6 at a time, with one retry on a
   timeout, a network failure, or status 408, 429, 500, 502, 503 or 504.
3. The stream stops at its end, or when the load no longer needs it. It also stops at the first problem:
   - an error status, or a content type other than `application/vnd.ipld.car`;
   - a root other than the record's CID, or a block that does not match its CID;
   - the archive limits;
   - less than 1 MiB in a 10 s window. The windows start at the request, so the first one includes the first byte.
4. The stream is not asked for again. A block that it brought is not fetched again, but a block can arrive both ways
   after the wait. The site is assembled in order.

- Why 2 s: in one client's measurements, a cached CAR sent its first byte in 150 ms and 24 MB in 1 s. An uncached CAR
  came at a few KB/s to 500 KB/s.
- The stream carries at most 64 MiB. Larger sites get the rest block by block.
- `FETCH_CAR` in `src/products/archive/gateway.ts` turns the stream off.
- Blocks from the host's own origin (the Browse seed, Contact Chat's worker) are read block by block, with no CAR stream
  and no wait. A static server has no CAR.
- The iOS app asks for a CAR with the same header. The Android app uses `?format=car` for a website, but plain
  `/ipfs/<cid>` for an app archive.

### Integrity

- **Bytes are checked.** Every archive chunk is checked against the record's CID, and every inner block against its own
  CID. A website is checked from the record's CID down. So the gateway is not trusted. The inner check proves only that
  the site is consistent. The outer check ties it to the name.
- **The record is trusted.** The CID comes from one RPC node. It is not a light-client-verified read.
- The cache is written only after the check. The service worker serves it with no second check.

### Limits

The unpacker follows the CARv1, dag-pb and UnixFS specifications. It refuses names that climb out of the site, duplicate
names, sharded directories, symlinks and files whose sizes disagree.

| Limit                             | Value     |
| --------------------------------- | --------- |
| Archive                           | 64 MiB    |
| Files, counted once per reference | 128 MiB   |
| Files                             | 10 000    |
| Blocks                            | 50 000    |
| Block size                        | 4 MiB     |
| Directory depth                   | 32 levels |
| Path length                       | 1 KiB     |

Loading also stops after a deadline.

### Load summary

- The loader posts status lines (`{type: "truapi-sandbox", state, detail}`) to the host page.
- Once per load it posts `{type: "truapi-sandbox-load", summary}` (`src/products/load-observer.ts`) for the
  [Products view](inspector.md#products): outcome, phase times, file counts and bytes, and what the stream and block
  reads did. It holds metadata only.
- The host reads it within bounds, assigns it to the frame that posted it, and keeps only the first one per frame. The
  product in that frame is on the same origin and could post one too.
- The cache marker holds the file count, bytes and largest file. So a cached load reports them without reading files.

## The Browse seed

A build ships the Browse app (`browse.paseo`) beside the page, so the first open does not wait on the gateway.

- `browse-seed-plugin.ts` resolves the name, reads and checks every block, unpacks the archive, and writes the blocks at
  `/ipfs/<cid>`. `browse-seed.json` names the record, CID, network, fetch time, size and blocks.
- The seed may add at most 16 MiB. Browse was 5.8 MB in 10 blocks when this was set.
- **It is a cache, not a source.** Opening Browse still resolves the name on chain. Only when the record names the
  seeded CID does the loader read the seed, and it still checks every block. Otherwise the gateway serves, as for any
  name. When the chain cannot be read, the open fails.
- `WEB_SIGNING_SEED_BROWSE`: a build seeds unless it is `0`, and fails if it cannot fetch the seed. The dev server seeds
  only when it is `1`. A build under another base has no seed, and `1` with such a base fails.
- `src/products/seed.ts` reads the seed in the host.

## Links in the product

- The service worker answers every request of its page. So a root-relative link, such as `/assets/app.js`, is read from
  the archive. A relative link resolves under the mount. Other origins go to the network.
- The frame leaves the mount on a navigation to a root-relative document (`<a href="/about">`, `location.assign("/x")`),
  or on a reload after `history.pushState` moved there.
- `location.pathname` is the mount path. A router that expects `/` needs hash routing or a base from the location.

## Loading and failures

- A thin line at the top of the app shows the phase ("Looking up name…", "Loading…"), with no percentage. The frame is
  `aria-busy`. The loader itself shows nothing: no white page, address or CID.
- When the loader fails, the app shows **This app could not open** once, with **Details** (the Product tab) and
  **Close**.
- A failed lookup leaves the open product as it is. After a successful lookup, the old product closes. A later failure
  leaves no product open.
- Closing or leaving a product ends its loading.
