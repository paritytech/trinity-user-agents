---
"@parity/truapi-provider": patch
---

The Rust crate can build a provider for a Trinity stack from its descriptor and the light specs a host hands over,
through `LightStack` and `EmbeddedChainProviderBuilder::stack`. The JavaScript API is unchanged. The WebAssembly bundle
grows by about 3%, because it now carries the descriptor checks.
