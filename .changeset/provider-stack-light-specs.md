---
"@parity/truapi-provider": minor
---

A provider can run the chains of a Trinity stack. `builder.setStack(link, descriptorBytes)` takes the stack's link and
its descriptor as served, and `builder.addLightSpec(genesisHash, specBytes)` takes each chain's light spec, which the
host fetches from the descriptor's `lightSpec.url`. The provider checks every spec against the descriptor's `sha256`
and the chain's genesis hash, syncs each parachain through the stack's relay, and runs the statement-store protocol
only on the chains the descriptor marks. `provider.addLightSpec` adds a spec after `build`. A stack's provider does not
resolve chains from the bundled catalog.

Breaking: the storage client is called as `load(genesisHash, stack)` and `save(genesisHash, blob, stack)`. `stack`
identifies the stack and is `undefined` without one. A store that serves more than one stack must keep a blob per
stack and genesis hash. What is stored now starts with a line naming the stack's `generation`, so a blob stored by an
earlier version is ignored once and the chain syncs from its checkpoint before the next save replaces it.
