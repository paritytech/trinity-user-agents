---
"@parity/truapi-host": patch
---

Rust embedders derive from the user's root entropy through `truapi::host_logic::root_entropy::RootEntropy`, built once from the bytes a host hands over, with each derivation a method on it. No function takes the entropy as `&[u8]`. Derived keys and addresses are unchanged.
