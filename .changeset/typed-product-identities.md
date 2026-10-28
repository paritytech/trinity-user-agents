---
"@parity/truapi": major
"@parity/truapi-host": major
---

Protocol and host APIs accept `ProductId` values for product identities. The TypeScript client exports a string brand and non-validating `asProductId` helper, with no WASM dependency. Host worker runtimes expose Rust-backed `parseProductId`; Swift and Kotlin construct immutable, native-owned values through the same Rust parser.

Kotlin `ProductId.parse` and `ProductId.fromName` return `Result<ProductId>`, carrying a `ProductIdException` on invalid input. Callers must explicitly recover or propagate the failure.

Kotlin `ProductId` is independent of Android parceling. Navigation payloads use an app-owned `Parceler<ProductId>` that writes the string value and validates it through Rust when restoring.

Rust consumers import `ProductId`, `ProductIdError` and `PRODUCT_ID_MAX_BYTES` from the `truapi` crate root; these version-independent items are not exported through `truapi::latest`.

Callers must pass typed IDs to product configuration, callbacks, proof contexts, registry queries, cross-product storage and NFT purse ownership. Valid IDs retain string wire and storage encodings. Malformed IDs in typed protocol fields fail decoding before dispatch. Persisted permission scopes and manifest labels keep their existing encodings.

Native product-link and executable-name operations use shared Rust helpers. Product links must match the host-provided network suffix, including `.li` links. Both native hosts stop content loading on product-resolution failures or missing executables.

The shared host execution enum is named `ExecutableKind` in Rust, Swift, Kotlin and TypeScript. Host consumers must update imports from `ProductExecutionKind`; its variants and wire encoding are unchanged.
