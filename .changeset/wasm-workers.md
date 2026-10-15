---
"@parity/truapi": patch
---

Experimental: a product worker can be a Rust crate compiled to `wasm32-unknown-unknown`, written against the new `truapi-guest-api` crate. The core runs it in process for one product, behind the `wasm-worker` feature, and answers each TrUAPI import with a direct call on that product's trait implementation. The CLI runs one with `/worker <wasm-path>` as the selected product.
