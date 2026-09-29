---
"@parity/truapi-host": minor
---

Add the product-scoped PolkaVM application runtime and its version-two guest transport. Route host capabilities through
runtime authority checks and preserve the existing consent requirements for signing and resource allocation.

Keep the SDK usable in no-std guest builds, including Pocket message types.

Preserve that guest boundary with the consolidated `truapi` crate: host API
traits use the `host-api` feature, and native/browser packaging explicitly
requests its shared or static library artifacts.

Build each iOS XCFramework slice with its own `cargo rustc` invocation.
Explicit static-library output cannot be combined with multiple target triples
in one invocation.

Qualify callback contracts through executable codec and WASM checks rather than
full-source declaration snapshots, while retaining deterministic code generation.

Preserve incoming-payment ownership and native Coinage ledger records when
migrating either the historical Chat store or current main's iOS store to the
combined model. Retain both historical model variants for migration detection.
