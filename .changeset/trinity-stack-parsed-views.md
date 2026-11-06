---
"@parity/trinity-stack": patch
"@parity/truapi": patch
---

The `trinity-stack` Rust crate reads a validated descriptor's values as bytes: `Chain::genesis` returns the genesis hash, `Descriptor::service` the service of a kind, and `Descriptor::dotns_address` a dotNS address. They decode hex that validation already checked, and panic only on a descriptor that skipped validation. The wasm API of `@parity/trinity-stack` is unchanged.
