---
"@parity/truapi-host": minor
---

The mock host can report chains it does not serve as unsupported. `featureSupported: "served"` answers `featureSupported` from `supportedChains`, as a real host does, so a product's "network unavailable" path is reachable on a host that serves no chains. The default stays `true`, and `"served"` becomes the default in a later breaking release. The Rust `MockConfig::only_served_chains` does the same.
