---
"@parity/truapi-host": patch
---

Rust embedders hold the user's root entropy as `truapi::host_logic::root_entropy::RootEntropy`, built once from the bytes a host hands over. Every derivation is a method on it, and local session activation takes it in place of raw bytes. The wasm and UniFFI entry points still take bytes. Derived keys and addresses are unchanged.
