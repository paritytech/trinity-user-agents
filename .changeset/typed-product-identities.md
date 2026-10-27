---
"@parity/truapi": major
"@parity/truapi-host": major
---

Protocol and host APIs accept `ProductId` values for product identities. The TypeScript client exports a string brand and non-validating `asProductId` helper, with no WASM dependency. Host worker runtimes expose Rust-backed `parseProductId`; Swift and Kotlin construct immutable, native-owned values through the same Rust parser.

Callers must pass typed IDs to product configuration, callbacks, proof contexts, registry queries, cross-product storage and NFT purse ownership. Valid IDs retain string wire and storage encodings. Malformed IDs in typed protocol fields fail decoding before dispatch. Persisted permission scopes and manifest labels keep their existing encodings.

Native product-link and executable-name operations use shared Rust helpers. Product links must match the host-provided network suffix, including `.li` links. iOS propagates product-resolution failures instead of loading fallback content.
