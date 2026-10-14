# uniffi-bindgen-cli

Thin CLI wrapper around `uniffi::uniffi_bindgen_main()` for generating native bindings (Swift and Kotlin) from UniFFI inputs in this workspace.

This crate exists so TrUAPI's native host SDKs (`android/truapi-host`, `ios/truapi-host`) can regenerate bindings via workspace-local tooling rather than relying on a globally installed `uniffi-bindgen`.

It does not add custom logic. It forwards directly into UniFFI's standard CLI entry point.

## Usage

```bash
cargo run -p uniffi-bindgen-cli -- generate \
  --library target/debug/libtruapi.so \
  --language kotlin \
  --out-dir android/truapi-host/src/main/kotlin/generated
```

Swift bindings land in `target/uniffi-swift-out/` (via `make uniffi` from the
repo root). The CLI emits all three files into one directory;
`ios/truapi-host/scripts/rebuild.sh` copies them into the Swift package,
renaming the modulemap to `module.modulemap` and colocating it with the header
so SwiftPM's `systemLibrary` target picks them up.

See `uniffi-bindgen --help` for the full CLI surface.
