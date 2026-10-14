# @parity/truapi-provider

## 0.3.1

### Patch Changes

- b92e186: `ChainProviderBuilder.setConnectionTypes({ secure, localhost, unsecure })` limits the kinds of connection the
  light client opens to peers: `wss://`, plain `ws://` to localhost, and plain `ws://` to any other peer. Each defaults
  to `true`; a page served over `https` can pass `{ unsecure: false }`, since the browser blocks those dials there as
  mixed content.
- b92e186: The bundled Paseo and previewnet relay chain specs carry warp-sync checkpoints at blocks 1264462 and 158908,
  so a cold-start light client warp-syncs from a recent finalized block.

## 0.3.0

### Minor Changes

- 715742a: A light-client connection holds the requests it is sent until smoldot first reports the chain as synced, then
  forwards them in the order they arrived, so every request is answered by a chain that has caught up. `chainSpec_v1_*`
  queries, `statement_*` and `bitswap_*` calls, and the `lifecycle_unstable_*` subscription, which reports the sync
  itself, are forwarded straight away. Held requests count against the 1024-frame budget of a connection, so a chain
  that never syncs refuses further requests with "light client response queue full".

  `provider.lifecycle(genesisHash)` watches the sync progress of a chain something is connected to. Each `next()` on the
  returned watch resolves with a `ChainLifecycle`: the phase (`connecting`, `syncing` with `at` and `target`, or
  `ready`), the connected peer count, and the health (`ok`, or `stalled` for `noPeers` or `noProgress`). The first call
  resolves with the current state, later ones with each change, and `undefined` after `close()` or once nothing holds
  the chain.

### Patch Changes

- de343d4: The `truapi-host` CLI previewnet preset and truapi-provider's previewnet catalog carry the relay, Asset Hub,
  Bulletin and People genesis hashes previewnet reports after its latest reset, and the bundled previewnet chain specs
  match them.

## 0.2.2

### Patch Changes

- a8fd3a0: The browser bundle is 4.25 MiB raw and 0.94 MiB brotli, down from 5.31 MiB and 2.01 MiB. smoldot's secp256k1
  multiplication tables are built the first time a runtime call uses a secp256k1 host function, in about 10 ms, instead
  of shipping as 1 MiB of precomputed points.

## 0.2.1

### Patch Changes

- a0387c9: The embedded light client answers `statement_subscribeStatement` the way a full node does: the subscription
  id is followed immediately by a snapshot batch carrying `remaining: 0`, empty when nothing matches, so a client that
  waits for the end of the replay before issuing its first query resolves rather than hanging. Live notifications still
  omit `remaining`, keeping a snapshot distinguishable from a gossiped statement.

## 0.2.0

### Minor Changes

- 087bdf6: The embedded light client holds at most 32 connections at once. A `connect` past that is refused with "the
  light client already holds 32 connections" instead of adding another chain, request queue, response stream and frame
  channel that nothing will release. Closing a connection hands its slot back.

  The ceiling is a backstop against connections a consumer never closes, not a budget to spend: the bundled catalog
  resolves eight chains, so one reused connection per chain stays well under it.
