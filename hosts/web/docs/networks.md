# Stacks and chain access

## Stacks

Account → **Stack** selects the tab's Trinity stack. The host applies it by reloading the tab, before the core starts.

| Stack         | TLD        | Descriptor                                                                                   |
| ------------- | ---------- | -------------------------------------------------------------------------------------------- |
| Paseo Next v2 | `.paseo`   | `https://stack-paseo-next.parity-testnet.parity.io/.well-known/trinity-stack.json` (default) |
| Previewnet    | `.testnet` | `https://previewnet.substrate.dev/.well-known/trinity-stack.json`                            |
| PCF Devnet    | `.dot`     | `https://stack.api.polkadotcommunity.foundation/.well-known/trinity-stack.json`              |

- The choice is per tab. A tab with no choice runs on Paseo Next v2.
- The descriptor is the source of everything it carries: the menu name (`name`), the chains, genesis hashes, RPC nodes,
  TLD, the network `getChainInfo` reports to products (`network`, `paseo` for all three), the DotNS resolver, the IPFS
  and web gateways and the identity backend. The host keeps no copy of these.
- `src/platform/network-config.ts` holds only what a descriptor does not carry: each built-in's pinned link and bundled
  copy, the namespace its state is kept under, its best-block posting candidate, the networks whose chain specs the
  provider bundles (which also sets each chain's default backend) and the resolver's storage slots.
- Storage, history, the learned username, and the tab's wallet and open product are per stack. Nothing from one stack is
  read on another. The built-in ids (`paseo`, `previewnet`, `pcf-devnet`) are those namespaces, so Paseo's and
  Previewnet's storage keys and topics stay as they were.
- Recovery phrases are not per stack. One wallet can sign in on any of them. Stacks with the same TLD derive the same
  accounts from it.
- PCF Devnet runs on Paseo's chains under Polkadot's `dot` TLD: a phrase derives the same identity there as on Polkadot.
  Use test phrases only. Its chains are not in the provider's catalog, so every one runs on its RPC node.
- Before the core starts, each RPC node is asked for its genesis hash, and People also for its `NetworkSuffix`, all at
  once and while the provider starts. A wrong answer stops the boot; a node that does not answer within 5 s is logged
  and fails on first use. This catches a stale descriptor, not a hostile node.

## Stack descriptors

- The build bundles the signed copies the CLI ships, `rust/crates/truapi-host-cli/stacks/`, which
  `scripts/update-stacks.sh` refreshes. Each must verify under the key in the stack's link, which `network-config.ts`
  pins.
- A tab starts from the newest copy it has, bundled or stored, without waiting on the network
  (`src/platform/stacks.ts`). After boot it fetches the published descriptor once. A newer copy this host can run is
  kept in `localStorage`, one per stack, and used from the next load; the Activity view says when it changes something
  the host reads. The running tab never switches copy. `WEB_SIGNING_STACK_REFRESH=0` turns the fetch off, as the offline
  browser tests do.
- A copy is refused when it does not verify under the link's key, is older than the one in use (signed stacks only), is
  past `expiresAt`, changes the stack's `id` or `tld`, uses a TLD the core does not serve (`dot`, `paseo`, `testnet`),
  or lacks the Relay, Asset Hub, People and Bulletin chains, the DotNS content resolver, an IPFS gateway at `/ipfs/` or
  a web gateway.
- The core still takes genesis hashes and the TLD, read from the copy in use. The bytes are kept with the config for
  when it takes the stack itself.

## Custom stacks

Account → Stack → **Add a stack** takes a descriptor URL or a stack link (`polkadotapp://stack?key=…&url=…`).

- **Trust.** A link with a key pins the stack to that key: every later copy must be signed by it. A plain URL pins
  nobody: whoever serves it decides the chains and services, and can roll the descriptor back. A signature says who
  published a stack, not that they are trustworthy. The host shows the signer, or that there is none, before the user
  adds the stack, and in the menu while it runs.
- **Check.** The host reads the descriptor from this page, so its server must allow CORS. It refuses one that does not
  verify, is expired or breaks a rule above. It then asks each RPC node which chain it serves and refuses a stack whose
  node answers wrongly. A custom stack may reuse any TLD the core serves and any chain, Polkadot's and the built-ins'
  included, so forks, mirrors and copies run side by side, each under its own namespace.
- **Built-ins.** A URL or key of a built-in stack selects the built-in, with its pinned key.
- **Namespace.** A custom stack's state is kept under `custom-` and a hash of its key, or of its URL when it has none,
  never under the descriptor's `id`.
- **Off on a custom stack:** the light client (every chain on its RPC node), the wallet backup (its pointer derives from
  the wallet and the People genesis, so stacks sharing a People chain would compete for it) and the dev server's
  username search (which reaches only the built-ins' identity backends).
- **Shared accounts.** A custom stack derives the same accounts as the built-in on its TLD. A signature not bound to a
  chain, such as a statement, is valid on both. The summary says so.
- **Forget this stack** removes it from the menu. Its state stays in the browser under its namespace.
- A custom stack that can no longer run (expired, or its kept copy refused) stops the page at load. The tab stays on it
  and the core does not start. Account says why; choose another stack there, or reload to try again. Meanwhile the host
  fetches the stack's published descriptor once and keeps a copy that runs for the next load.

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

- It starts from the provider's bundled catalog entry named by the stack's descriptor `id` (`paseo-next-v2` or
  `previewnet`), whose chains are those of the bundled descriptor (a unit test holds that). A custom stack never starts
  from the catalog: a fork keeps its source's genesis hashes, and the catalog's specs would sync the source. A stored
  copy whose chains restarted after the build has no chain spec there, so its chains run on their RPC nodes whatever the
  choice, and the Activity view says so.
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
