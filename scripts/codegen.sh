#!/usr/bin/env bash
# Regenerate js/packages/truapi/src/generated/* from rust/crates/truapi.
#
# Pipeline:
#   1. cargo +<pinned nightly> doc -p truapi --no-default-features
#      -> target/doc/truapi.json, the protocol definitions alone, kept as
#      target/doc/truapi_protocol.json
#   2. cargo run -p truapi-codegen -- --input target/doc/truapi_protocol.json
#                                     --output js/packages/truapi/src/generated
#                                     --rust-output rust/crates/truapi/src/generated
#      The runtime includes the dispatcher, so it has to exist before the
#      runtime can be documented.
#   3. cargo +<pinned nightly> doc -p truapi -p truapi-provider
#      -> target/doc/{truapi,truapi_provider}.json, with the runtime
#   4. cargo run -p truapi-codegen -- --input target/doc/truapi_protocol.json
#                                     --output js/packages/truapi/src/generated
#                                     --playground-output js/packages/truapi/src/playground
#                                     --client-examples-output playground/test/generated/examples
#                                     --rust-output rust/crates/truapi/src/generated
#                                     --platform-input target/doc/truapi.json
#                                     --platform-input target/doc/truapi_provider.json
#                                     --platform-ts-output js/packages/truapi-host/src/generated
#                                     --platform-wasm-adapter-output js/packages/truapi-host/src/generated
#                                     --platform-rust-output rust/crates/truapi/src/wasm
#
# The codec version is not passed: it defaults to `truapi::WIRE_CODEC_VERSION`,
# so the generated client and the host's handshake derive it from one place.
#
# The client surface defaults to the latest wire version any versioned
# wrapper exposes; pass `--client-version V<N>` to pin to an older one.
#
# Run from the repo root.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# The dated nightly CI runs, from nightly-toolchain; TRUAPI_NIGHTLY_TOOLCHAIN
# overrides it.
NIGHTLY_TOOLCHAIN="${TRUAPI_NIGHTLY_TOOLCHAIN:-$(head -n1 "$ROOT/nightly-toolchain" | tr -d '[:space:]')}"

# Homebrew LLVM on DYLD_LIBRARY_PATH makes rustc and rustdoc load a mismatched
# libLLVM, which dies with SIGSEGV in initialize_available_targets.
unset DYLD_LIBRARY_PATH

RUSTDOCFLAGS="${RUSTDOCFLAGS:-} -D warnings -Z unstable-options --output-format json"
export RUSTDOCFLAGS

cargo +"$NIGHTLY_TOOLCHAIN" doc -p truapi --no-deps --no-default-features
cp target/doc/truapi.json target/doc/truapi_protocol.json
cargo run -p truapi-codegen -- \
  --input target/doc/truapi_protocol.json \
  --output js/packages/truapi/src/generated \
  --rust-output rust/crates/truapi/src/generated
cargo +"$NIGHTLY_TOOLCHAIN" doc -p truapi -p truapi-provider --no-deps
cargo run -p truapi-codegen -- \
  --input target/doc/truapi_protocol.json \
  --output js/packages/truapi/src/generated \
  --playground-output js/packages/truapi/src/playground \
  --client-examples-output playground/test/generated/examples \
  --rust-output rust/crates/truapi/src/generated \
  --platform-input target/doc/truapi.json \
  --platform-input target/doc/truapi_provider.json \
  --platform-ts-output js/packages/truapi-host/src/generated \
  --platform-wasm-adapter-output js/packages/truapi-host/src/generated \
  --platform-rust-output rust/crates/truapi/src/wasm \
  --explorer-output js/packages/truapi/src/explorer

rustfmt +"$NIGHTLY_TOOLCHAIN" --edition 2024 \
  rust/crates/truapi/src/generated/dispatcher.rs \
  rust/crates/truapi/src/generated/wire_table.rs \
  rust/crates/truapi/src/wasm/generated_bridge.rs

node scripts/regen-explorer-versions.mjs

# --no: use the installed prettier, never resolve or fetch one.
npm exec --no -- prettier --write \
  "js/packages/truapi/src/generated/**/*.ts" \
  "js/packages/truapi/src/playground/**/*.ts" \
  "js/packages/truapi/src/explorer/**/*.ts" \
  "playground/test/generated/examples/**/*.ts" \
  "js/packages/truapi-host/src/generated/**/*.ts"

# Rebuild dist/ so downstream consumers (in particular the playground,
# which picks up @parity/truapi via yarn 1.x file: snapshot) see the
# regenerated bindings without a separate npm run build step.
#
# The build runs twice: the first pass emits the freshly-generated client
# sources to dist/, then `bundle-truapi-dts.mjs` snapshots dist/*.d.ts into
# src/playground/codegen/truapi-dts.ts so Monaco can register the package as
# an ambient module without HTTP fetches. The second pass compiles the new
# truapi-dts.ts itself.
if [ "${TRUAPI_SKIP_PACKAGE_BUILD:-0}" != "1" ]; then
  # npm workspaces hoist node_modules to the repo root, so check there.
  if [ ! -d node_modules ]; then
    npm ci
  fi
  npm run build --prefix js/packages/truapi
  node scripts/bundle-truapi-dts.mjs
  npm run build --prefix js/packages/truapi
fi

echo "Generated client at js/packages/truapi/src/generated/"
echo "Generated playground metadata at js/packages/truapi/src/playground/codegen/"
echo "Generated client examples at playground/test/generated/examples/"
echo "Generated Rust dispatcher at rust/crates/truapi/src/generated/"
echo "Generated host-callbacks WASM adapter at js/packages/truapi-host/src/generated/"
echo "Generated Rust WASM bridge at rust/crates/truapi/src/wasm/generated_bridge.rs"
