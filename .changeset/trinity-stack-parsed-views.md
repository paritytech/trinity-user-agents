---
"@parity/trinity-stack": patch
"@parity/truapi": patch
---

The `trinity-stack` Rust crate reads a validated descriptor's values without decoding them again: `Chain::genesis` returns the genesis hash as bytes, `Descriptor::service` the service of a kind, and `Descriptor::dotns_address` a dotNS address as bytes. The wasm API of `@parity/trinity-stack` is unchanged.
