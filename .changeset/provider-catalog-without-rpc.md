---
"@parity/truapi-provider": minor
---

A provider with an `addRpcChain` registration connects only to the chains registered on it. `connect` with any other genesis hash fails as unknown instead of resolving it from the bundled catalog, so a host on remote nodes never starts the light client by accident. A provider with no remote node still resolves every bundled network from its genesis hash alone. A host that mixes the two calls `addNetwork` first and `addRpcChain` after it: `addNetwork` registers every chain of the network, and a later registration for a genesis replaces an earlier one, so the other order puts the remote chains back on the light client.
