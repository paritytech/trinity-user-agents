#!/usr/bin/env bash
# Regenerate the TrUAPIProvider package build outputs in place:
#   * truapi_provider.xcframework (Binaries/), device and simulator slices
#   * uniffi-generated Swift bindings
#     (Sources/TrUAPIProvider + Sources/truapi_providerFFI)
#
# Run after changing the crate's uniffi surface or refreshing the bundled chain
# specs. The outputs are gitignored, so the package's Swift target does not
# exist until this has run.
# Usage: ./scripts/rebuild.sh [--sim-only]
#
# --sim-only drops the device slice for a faster loop; publish.sh rejects the
# result, since a release asset without a device slice cannot ship.
set -euo pipefail

PACKAGE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TRUAPI_ROOT="$(cd "$PACKAGE_ROOT/../.." && pwd)"
cd "$TRUAPI_ROOT"

PROFILE="${PROFILE:-release}"
LIB=libtruapi_provider.a

SLICES=(aarch64-apple-ios-sim aarch64-apple-ios)
[ "${1:-}" = "--sim-only" ] && SLICES=(aarch64-apple-ios-sim)

CARGO_FLAGS=(-p truapi-provider --lib --no-default-features --features uniffi)
[ "$PROFILE" = release ] && CARGO_FLAGS+=(--release)

rustup target add "${SLICES[@]}" >/dev/null 2>&1 || true

# One invocation carrying every slice, so the target graphs are scheduled
# together rather than one draining before the next starts.
echo "==> building ${SLICES[*]} ($PROFILE)"
cargo build "${CARGO_FLAGS[@]}" "${SLICES[@]/#/--target=}"

# Bindings come from the workspace bindgen so every package in this repo
# generates them the same way.
UNIFFI_OUT="$TRUAPI_ROOT/target/uniffi-provider-swift-out"
rm -rf "$UNIFFI_OUT" && mkdir -p "$UNIFFI_OUT"
echo "==> generating Swift bindings"
cargo run -q -p uniffi-bindgen-cli -- generate \
    --library "target/${SLICES[0]}/$PROFILE/$LIB" \
    --language swift \
    --out-dir "$UNIFFI_OUT"

PROVIDER_UNIFFI_OUT="$UNIFFI_OUT" sh "$PACKAGE_ROOT/scripts/sync-bindings.sh"

# The xcframework carries the headers; the systemLibrary target above reads the
# committed copies, so both must come from this same generation.
HEADERS="$TRUAPI_ROOT/target/uniffi-provider-headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$UNIFFI_OUT/truapi_providerFFI.h" "$HEADERS/"
cp "$UNIFFI_OUT/truapi_providerFFI.modulemap" "$HEADERS/module.modulemap"

OUT="$TRUAPI_ROOT/target/truapi_provider.xcframework"
rm -rf "$OUT"
ARGS=()
for target in "${SLICES[@]}"; do
    ARGS+=(-library "$TRUAPI_ROOT/target/$target/$PROFILE/$LIB" -headers "$HEADERS")
done
echo "==> packaging truapi_provider.xcframework"
xcodebuild -create-xcframework "${ARGS[@]}" -output "$OUT"

PROVIDER_XCFRAMEWORK="$OUT" sh "$PACKAGE_ROOT/scripts/stage-xcframework.sh"

echo "done."
echo "Build against it with TRUAPI_PROVIDER_USE_LOCAL_BINARY=1; publish with scripts/publish.sh."
