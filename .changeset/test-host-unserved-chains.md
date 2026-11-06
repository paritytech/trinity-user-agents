---
"@parity/truapi-host": minor
---

The mock host can report chains it does not serve as unsupported. `onlyServedChains: true` answers `featureSupported`
from `servedChains`, as a real host does, so a product's "network unavailable" path is reachable on a host that serves
no chains. It defaults to `false`, and becomes the default in a later breaking release. The Rust
`MockConfig::only_served_chains` is the same option.
