#!/bin/sh
# Copy the uniffi-generated Swift bindings from target/uniffi-swift-out into the
# TrUAPIHost package, stripping the trailing whitespace UniFFI's templates emit.
#
# The bindings are gitignored build outputs, so the package's Swift targets do
# not exist until this runs.
#
# Requires `make uniffi` (or `make xcframework`, which depends on it) to have run
# first; this script generates nothing itself.
set -eu

PACKAGE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TRUAPI_ROOT="$(cd "$PACKAGE_ROOT/../.." && pwd)"
UNIFFI_OUT="$TRUAPI_ROOT/target/uniffi-swift-out"

NAMESPACES="truapi truapi_platform truapi_server"

if [ ! -d "$UNIFFI_OUT" ]; then
    echo "error: $UNIFFI_OUT is missing." >&2
    echo "Run 'make uniffi' at the repo root first; this script only copies." >&2
    exit 66
fi

for namespace in $NAMESPACES; do
    mkdir -p "$PACKAGE_ROOT/Sources/TrUAPIHost" \
        "$PACKAGE_ROOT/Sources/${namespace}FFI/include"
    cp "$UNIFFI_OUT/${namespace}.swift" \
        "$PACKAGE_ROOT/Sources/TrUAPIHost/${namespace}.swift"
    cp "$UNIFFI_OUT/${namespace}FFI.h" \
        "$PACKAGE_ROOT/Sources/${namespace}FFI/include/${namespace}FFI.h"
    # The SwiftPM systemLibrary target looks for module.modulemap by name.
    cp "$UNIFFI_OUT/${namespace}FFI.modulemap" \
        "$PACKAGE_ROOT/Sources/${namespace}FFI/include/module.modulemap"
    perl -pi -e 's/[ \t]+$//' \
        "$PACKAGE_ROOT/Sources/TrUAPIHost/${namespace}.swift" \
        "$PACKAGE_ROOT/Sources/${namespace}FFI/include/${namespace}FFI.h" \
        "$PACKAGE_ROOT/Sources/${namespace}FFI/include/module.modulemap"
done

echo "Bindings synced into $PACKAGE_ROOT/Sources"
