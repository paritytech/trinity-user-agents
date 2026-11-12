#!/bin/sh
# Copy the uniffi-generated Swift bindings from target/uniffi-provider-swift-out
# into the TrUAPIProvider package.
#
# The bindings are gitignored build outputs, so the package's Swift targets do
# not exist until this runs. Split out of rebuild.sh so a caller that only needs
# the sources (the release tag, CI's compile gate) does not have to build an
# xcframework, which needs Xcode and the iOS targets.
#
# Requires `make provider-swift` (or rebuild.sh, which generates the same
# directory) to have run first; this script generates nothing itself.
set -eu

PACKAGE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TRUAPI_ROOT="$(cd "$PACKAGE_ROOT/../.." && pwd)"
UNIFFI_OUT="${PROVIDER_UNIFFI_OUT:-$TRUAPI_ROOT/target/uniffi-provider-swift-out}"

if [ ! -d "$UNIFFI_OUT" ]; then
    echo "error: $UNIFFI_OUT is missing." >&2
    echo "Run 'make provider-swift' at the repo root first; this script only copies." >&2
    exit 66
fi

mkdir -p "$PACKAGE_ROOT/Sources/TrUAPIProvider" \
    "$PACKAGE_ROOT/Sources/truapi_providerFFI/include"
cp "$UNIFFI_OUT/truapi_provider.swift" \
    "$PACKAGE_ROOT/Sources/TrUAPIProvider/truapi_provider.swift"
cp "$UNIFFI_OUT/truapi_providerFFI.h" \
    "$PACKAGE_ROOT/Sources/truapi_providerFFI/include/truapi_providerFFI.h"
# The SwiftPM systemLibrary target looks for module.modulemap by name.
cp "$UNIFFI_OUT/truapi_providerFFI.modulemap" \
    "$PACKAGE_ROOT/Sources/truapi_providerFFI/include/module.modulemap"

echo "Provider bindings synced into $PACKAGE_ROOT/Sources"
