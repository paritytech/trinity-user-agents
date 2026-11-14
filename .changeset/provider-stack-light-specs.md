---
"@parity/truapi-provider": major
---

A provider can run the chains of a Trinity stack. `builder.setStack(link, descriptorBytes)` takes the stack's link and
its descriptor as served, `builder.lightSpecs()` lists where each chain's light spec is published, and
`builder.addLightSpec(genesisHash, specBytes)` takes each spec the host fetched. The provider checks every spec against the descriptor's `sha256`
and the chain's genesis hash, syncs each parachain through the stack's relay, and runs the statement-store protocol
only on the chains the descriptor marks. `provider.addLightSpec` adds a spec after `build`. A stack's provider does not
resolve chains from the bundled catalog.

Breaking: the storage client is called as `load(genesisHash, stack)` and `save(genesisHash, blob, stack)`. `stack`
identifies the stack and is `undefined` without one. A store that serves more than one stack must keep a blob per
stack and genesis hash. A stack's provider starts each blob with a line naming the stack and its `generation` and
reads back only its own. A provider without a stack stores what it stored before.
