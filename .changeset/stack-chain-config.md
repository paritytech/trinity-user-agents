---
"@parity/truapi-host": major
---

The core takes its chain configuration from a Trinity stack descriptor. `ProductRuntimeConfig.stack` replaces `people`,
`bulletin`, `assetHub` and `networkSuffix`: `pointer` is the stack's link (`polkadotapp://stack?key=0x…&url=…`),
`descriptor` the bytes of the signed copy the host build bundles, and `allowsReservedValues` whether the build may run a
stack on Polkadot's genesis hashes or the `dot` TLD. The core verifies the copy against the pointer's key and refuses a
pointer without a key, a descriptor without a People or Bulletin chain, and a reserved value the build does not allow.
`getChainInfo` answers from the descriptor's `chains` and `network`, so the `supportedChains` host callback and the
`HostChainSet` and `HostChainEntry` types are removed. The test host runs a stack built from its parts:
`mockRuntimeConfig()` returns a `testStack` over the `MOCK_GENESIS` chains, which only the `wasm/testing` bundle reads,
`fromNetworks` and `liveChain` build it from their chains, and `MockHostConfig.servedChains` replaces `supportedChains`
as the genesis hashes `featureSupported` answers from under `onlyServedChains`.
