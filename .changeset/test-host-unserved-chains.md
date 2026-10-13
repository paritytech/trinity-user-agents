---
"@parity/truapi-host": major
---

The mock host answers `featureSupported` for a chain from the chains it reports serving. A chain outside `supportedChains` is now unsupported, where it used to be supported and then left silent, so a product's "network unavailable" path is reachable on a host started with no chains. The Rust `MockPlatform` answers the same way.

A suite that builds `mock.chainProxies` by hand for a chain its `supportedChains` does not list has to add that chain to `supportedChains`. Suites configured through `networks`, `chain` or `liveChain` already list it.
