# truapi-codegen

_Reads rustdoc JSON for the `truapi` crate and generates TypeScript clients and Rust codecs._

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](../../../LICENSE)

`truapi-codegen` keeps the generated client aligned with the Rust protocol definition. It reads rustdoc JSON, extracts the TrUAPI API surface, and writes:

- TypeScript types for every protocol type in `truapi`.
- TypeScript domain client classes for every unified trait.
- The TypeScript wire dispatch table.
- The Rust server dispatcher and transport-neutral wire table.
- The no-std Rust client method catalog when `--rust-client-output` is supplied.

## Generated output

Generated methods encode their payloads and pass complete wire frames to the transport. Every frame carries a SCALE request id, a `(trait, method)` byte pair, a `message_type` byte, and the leg's inline payload. Request responses encode `Result<Response, CallError<Error>>`; subscriptions deliver versioned items and terminate with `Result<(), CallError<Error>>`.

The Rust client catalog targets `truapi-client`: request markers implement `RequestMethod`, product-initiated streams implement `SubscriptionMethod`, and product-served streams such as `Renderer::render` implement `HostSubscriptionMethod`. Both subscription traits declare their domain error through `Error`. All legs share `MethodIds`; the message type distinguishes the leg.

## Architecture

The generator runs in three stages:

1. **Parse**: read JSON emitted by nightly rustdoc.
2. **Normalize**: extract the API model, including `#[wire_trait(id = N)]` and each method's `#[wire(id = N)]`.
3. **Emit**: generators write TypeScript clients and optional Rust dispatcher, wire-table, and client-catalog outputs.

Missing or duplicate wire ids fail generation. Trait id 255 is reserved for correlated protocol errors. One `(trait, method)` pair addresses a method whatever its shape: which leg a frame carries is the envelope's own `message_type` byte.

## CLI

```bash
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi/src/generated \
  --rust-output rust/crates/truapi/src/generated \
  --rust-client-output rust/crates/truapi-client/src/generated.rs
```

## Typical workflow

```bash
cargo +$(cat nightly-toolchain) rustdoc -p truapi -- -Z unstable-options --output-format json
cargo run -p truapi-codegen -- \
  --input target/doc/truapi.json \
  --output js/packages/truapi/src/generated \
  --rust-output rust/crates/truapi/src/generated \
  --rust-client-output rust/crates/truapi-client/src/generated.rs
```

The repo wraps both steps in [`scripts/codegen.sh`](../../../scripts/codegen.sh), which is what you should run from the repo root.

CI passes the generated TypeScript and Rust output to the Rust job. `TRUAPI_REQUIRE_GENERATED_TS=1 cargo test -p truapi --test wire_table_ts_parity` enforces actual Rust/TypeScript wire-table parity; `cargo test -p truapi-client` exercises client frame codecs after regeneration. Generated Rust source is not compared against checked-in dispatcher or wire-table text snapshots.

## When to run it

Run after any trait or type change in [`truapi`](../truapi/). If you only change runtime behavior without changing the protocol shape, regeneration is not needed.

## License

[MIT](../../../LICENSE)
