---
"@parity/truapi-host": patch
---

Shared host operations emit `host_runtime.<method>` trace names on both roles. Rust embedders construct `HostRuntime::<PairingHost>::new(...)` for a paired wallet or `HostRuntime::<SigningHost>::new(...)` for a local wallet.
