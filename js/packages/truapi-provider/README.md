# @parity/truapi-provider

Network transport for the TrUAPI `ChainProvider` contract: an embedded
[smoldot](https://github.com/smol-dot/smoldot) light client, or a remote
WebSocket JSON-RPC node, behind one API. This package is the WebAssembly build,
for browser and desktop (webview) hosts.

You connect to a chain by its genesis hash. A provider built for a Trinity stack
(`setStack`) runs the chains of the stack's descriptor from the light specs you
fetched for it. Without a stack, the bundled catalog provides the spec and relay
wiring of `paseo-next-v2` and `previewnet`. One light client is shared across all
connections, and its synced state is kept between launches, so a launch resumes
from finalized state instead of syncing from scratch. It holds at most 32 of them
at once and rejects a `connect` past that, so a client that leaks connections
fails instead of growing; closing one hands its slot back. Connections to a
remote node you registered with `addRpcChain` are not counted against it. A
provider with an `addRpcChain` registration connects only to chains registered
on it. The catalog does not resolve any other genesis hash for it, so a host on
remote nodes never starts the light client by accident.

## Usage

#### Web (JavaScript)

The published package is `wasm-bindgen` glue plus a `.wasm` binary. Instantiate
the module once per page or worker.

```js
import init, { ChainProviderBuilder } from "@parity/truapi-provider";
import wasmUrl from "@parity/truapi-provider/truapi_provider_bg.wasm?url";

await init({ module_or_path: wasmUrl });

const builder = new ChainProviderBuilder();
const chains = builder.addNetwork("paseo-next-v2");

const provider = builder.build();
const connection = await provider.connect(chains.relay);

connection.send(
  '{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_genesisHash","params":[]}',
);
const response = await connection.nextResponse(); // undefined once closed
connection.close();
```

To run a Trinity stack, give the builder the stack's link and its descriptor as
served, the same pair the host gives the core. The builder then lists where
each chain's light spec is published:

```js
const builder = new ChainProviderBuilder();
builder.setStack(link, descriptorBytes);
for (const { genesisHash, url } of builder.lightSpecs()) {
  const response = await fetch(url);
  if (!response.ok) continue; // the chain stays unknown; add its spec later
  builder.addLightSpec(genesisHash, new Uint8Array(await response.arrayBuffer()));
}
const provider = builder.build();
```

- `setStack` throws when the link is malformed or the descriptor is refused
  under it. A link with a key accepts only a descriptor signed with that key.
  The descriptor's `validUntil`, `expiresAt` and `sequence` are not read, so
  pass a copy a stack reader accepted.
- `addLightSpec` throws when the bytes differ from the descriptor's `sha256` or
  imply another genesis hash, and names the chain's role. Pass the bytes as
  served, because the digest covers them. A wrong spec is what a stale
  descriptor looks like after a wipe, so fetch the descriptor again before
  refusing the stack.
- A chain without its spec is unknown to the provider and `connect` rejects.
  The rest of the stack runs. `provider.addLightSpec(genesisHash, bytes)`
  supplies the spec later. A parachain also needs its relay's.
- The descriptor decides the relay every other chain syncs through, and which
  chains run the statement-store protocol. The bundled catalog is not used.
- Build one provider per stack, and a new one when the stack's `generation`
  changes. The light client keys a chain by its genesis hash, which a fork
  shares with its source.

Warm start is opt-in, because this package stores nothing itself. Hand
`setStorage` a client to storage you already own:

```js
builder.setStorage({
  async load(genesisHash, stack) {
    return (await myStore.get(`${stack ?? ""}:${genesisHash}`)) ?? null;
  },
  async save(genesisHash, blob, stack) {
    await myStore.put(`${stack ?? ""}:${genesisHash}`, blob);
  },
});
```

Both are called with a `0x`-prefixed lowercase genesis hash. `stack` identifies
the stack given to `setStack`: its key in hex when the link has one, otherwise
its normalised descriptor URL. It is `undefined` on a provider without a stack.
Keep a blob per stack and genesis hash, and return it unchanged. A stack's
provider starts each blob with a line naming the stack and its `generation`,
and reads back only its own, so a fork never resumes from its source's state
and a wiped stack starts fresh. A client that cannot
answer must reject rather than resolve `null`, which means "nothing stored yet"
and would let the next write replace good state. Without a client every chain
syncs from the checkpoint in the chain spec on every run.

`setDatabaseContent(genesisHash, blob)` seeds one chain for this run and beats
anything the client would load for it. `saveDatabase(genesisHash)` forces a write
at a moment the host chooses.

The light client dials peers over `wss://`, and over plain `ws://` to localhost
and to other hosts. A page whose browser blocks some of those can turn them off:

```js
builder.setConnectionTypes({ unsecure: false }); // mixed content on an https page
```

A light-client connection holds its requests until the chain first syncs, then
sends them in order. Chain-spec queries, statement-store and Bitswap calls are
answered at once. To show sync
progress, watch the chain once something is connected to it:

```js
const watch = provider.lifecycle(chains.relay);
for (let state; (state = await watch.next()); ) {
  // state.phase: { kind: "connecting" } | { kind: "syncing", at, target } | { kind: "ready" }
  // state.peers: connected peer count
  // state.health: { kind: "ok" } | { kind: "stalled", reason: "noPeers" | "noProgress" }
}
```

The first `next()` resolves with the current state, and each later one with the
next change. It resolves `undefined` after `watch.close()` or once nothing holds
the chain. A parachain goes from `connecting` straight to `ready`, so watch its
relay for warp sync progress. `lifecycle` throws for a chain nothing is
connected to.

## Native hosts

Android and iOS do not consume this package. The same `truapi-provider` crate is
published for them as its own artifacts over UniFFI — a `TrUAPIProvider` Swift
package and a `truapi-provider-android` AAR — exposing the same `ChainProvider`
contract with the same bundled catalog, so the wiring differs only in language.

## Building

The `dist/` bundle is generated and gitignored. Rebuild it from the Rust crate:

```bash
npm run build           # wasm-pack --target web, features "js networks"
```

`wasm-pack` is required (`cargo install wasm-pack`). Set `TRUAPI_WASM_PROFILE=dev`
for a fast unoptimized build. The repo's `make wasm` target rebuilds this bundle
alongside the host runtime.

## License

MIT AND Apache-2.0. See [LICENSE](LICENSE), [LICENSE-APACHE](LICENSE-APACHE), and
[NOTICE](NOTICE).
