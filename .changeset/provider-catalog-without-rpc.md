---
"@parity/truapi-provider": minor
---

A provider with an `addRpcChain` registration connects only to the chains registered on it. `connect` with any other genesis hash fails as unknown instead of resolving it from the bundled catalog, so a host on remote nodes never starts the light client by accident. A provider with no remote node still resolves every bundled network from its genesis hash alone. A host that mixes the two in one provider calls `addNetwork` for the networks it reaches on the light client.
