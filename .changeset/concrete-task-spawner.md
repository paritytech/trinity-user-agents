---
"@parity/truapi-host": patch
---

Bind native background tasks to the host's Tokio runtime and schedule browser tasks on the local WASM executor. Rust embedders construct `truapi::Spawner` with a runtime handle on native targets or `Default` on WASM, and submit futures with `spawn`.
