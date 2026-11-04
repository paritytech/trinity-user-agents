# Networks and chain access

## Networks

Account → **Network** selects the tab's network. The host applies it by reloading the tab, before the core starts.

| Network       | TLD        | Use                                                     |
| ------------- | ---------- | ------------------------------------------------------- |
| Paseo Next v2 | `.paseo`   | The default. Products for the current testnet are here. |
| PreviewNet    | `.testnet` | The product preview network. Names differ from Paseo's. |

- The choice is per tab. A tab with no choice runs on Paseo Next v2.
- Storage, history, the learned username, and the tab's wallet and open product are per network. Nothing from one
  network is read on another.
- Recovery phrases are not per network. One wallet can sign in on both.
- Each network's chains, RPC nodes, TLD, DotNS resolver and gateway are in `src/platform/network-config.ts`. The sources
  are the provider catalog, dotkit's `assets/envs.toml` and the signed Trinity stack descriptor
  (`paritytech/triangle-status` `environments.json`). The Paseo Next v2 values match dotli's
  `packages/config/src/network.ts`.

## Chain backends

Each chain runs on its RPC node or on the embedded light client (`@parity/truapi-provider`).

| Chain     | Default backend | Why                                                                                     |
| --------- | --------------- | --------------------------------------------------------------------------------------- |
| Asset Hub | RPC node        | As on the native hosts. The light client drops a PGAS allowance claim before broadcast. |
| People    | RPC node        | The node keeps a statement store. The light client has none.                            |
| Bulletin  | RPC node        | As on the native hosts.                                                                 |
| Relay     | Light client    | Only for products that ask for it. The host and the core do not use it.                 |

- `chains` in `network-config.ts` sets the defaults. `connectNetwork` takes an override per chain.
- **Account → Network → Chain connections** selects a backend per chain for this tab. The choice is saved per tab and
  network, and shows beside the running backend. **Reload to apply** reconnects the core, signs the wallet in again,
  restarts the workers and reopens the product. **Use defaults** clears the choice.
- The host trusts the answers of a chain on its RPC node.
- A DotNS name lookup always reads the Asset Hub RPC node over HTTP, whatever the Asset Hub backend.

## Light client

- It starts from the network's entry in the bundled catalog (`catalogNetwork`: `paseo-next-v2` or `previewnet`). The
  core's genesis hashes and the chain set reported to products come from the same entry.
- It keeps no state between loads. Each tab syncs from the chain spec checkpoint.
- In a production build, the light client allows only secure (`wss://`) peer connections. Its localhost and unsecure
  connection types are off (`hostConnectionTypes` in `src/platform/network.ts`). This keeps peers that advertise
  loopback or plain addresses from triggering Chrome's Local Network Access prompt. A dev server allows all three types,
  for local nodes.
- One light-client connection holds two `chainHead_v1_follow` subscriptions and refuses a third. The core runs every
  product's follows over one connection per chain. So `src/platform/follow-spread.ts` opens each follow on a connection
  with a free slot, and routes each `chainHead_v1_*` call to the connection that holds its follow.

## Preimages

- The host reads a product's preimage lookups (for example Browse's catalog and icons) from the network's Bulletin
  gateway, as the native hosts do.
- `src/products/preimage.ts` fetches `/ipfs/<cid>` for the raw blake2b-256 CID of the key. The core checks the bytes.
- No answer within 30 s, or no such preimage, gives a miss.
