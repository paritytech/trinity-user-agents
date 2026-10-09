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

Make native permission settings use canonical core decisions, including grants
already persisted before this update. Enumerate existing native core storage,
import legacy decisions only when no canonical record exists, and retain reset
tombstones so old legacy grants cannot return. Product-scoped revocation cancels
pending permission prompts, clears sibling one-use grants and closes live native
executions; a fresh execution can open normally under the updated decision.
Native callbacks must support key enumeration and permission-change notifications.
