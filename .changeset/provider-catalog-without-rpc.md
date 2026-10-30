---
"@parity/truapi-provider": minor
---

`connect` on a provider with an `addRpcChain` registration fails for any chain not registered on it, instead of starting the light client for it from the bundled catalog. A provider without one still reaches every bundled network by genesis hash. To mix the two, call `addNetwork` before `addRpcChain`: `addNetwork` registers all four chains of the network, and a later registration replaces an earlier one.
