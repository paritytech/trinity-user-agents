---
"@parity/truapi": minor
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

Centralize iOS permission presentation for consent prompts and app settings,
preserving the separate Chat and genesis-scoped JAM peer disclosures.
Make native permission settings use canonical core decisions, including grants
already persisted before this update. Enumerate existing native core storage,
import legacy decisions only when no canonical record exists, and retain reset
tombstones so old legacy grants cannot return. Product-scoped revocation cancels
pending permission prompts, clears sibling one-use grants and closes live native
executions; a fresh execution can open normally under the updated decision.
Native callbacks must support key enumeration and permission-change notifications.

Pin the optional native composition to PolkaVM host runtime `0.3.2-rc.9`
(`959ad63f7312a2f4598b9f718ccc2516927cbbff`) and compose the latest upstream
native SDK, including expanded-card and game callbacks, without weakening
canonical permission revocation or stale-prompt fencing.
The dependency license gate explicitly covers the runtime's MPL-2.0 Wasm compiler,
alongside the existing per-crate runtime and wire-protocol exceptions.
