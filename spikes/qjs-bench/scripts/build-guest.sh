#!/usr/bin/env bash
# Build the QuickJS guest for wasm32-wasip1 and, when binaryen is reachable
# through npx, an -Oz copy next to it. rquickjs-sys downloads wasi-sdk 24 into
# its OUT_DIR on first build unless WASI_SDK points at an install.
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add wasm32-wasip1 >/dev/null
cargo build -p qjs-guest --target wasm32-wasip1 --release

out=target/wasm32-wasip1/release
if npx -y -p binaryen@123.0.0 wasm-opt --version >/dev/null 2>&1; then
  npx -y -p binaryen@123.0.0 wasm-opt -Oz \
    --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
    "$out/qjs_guest.wasm" -o "$out/qjs_guest.opt.wasm"
fi
ls -l "$out"/qjs_guest*.wasm
