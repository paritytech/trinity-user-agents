#!/bin/sh
# Xcode flattens every slice's Headers into one include directory, so a
# slice-local module.modulemap collides with another xcframework's. The module
# comes from the committed Sources/truapi_providerFFI/include/module.modulemap,
# which must stay.
set -eu

PROVIDER_PACKAGE_ROOT="${PROVIDER_PACKAGE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
PROVIDER_XCFRAMEWORK="${PROVIDER_XCFRAMEWORK:-$(cd "$PROVIDER_PACKAGE_ROOT/../.." && pwd)/target/truapi_provider.xcframework}"
BINARIES="$PROVIDER_PACKAGE_ROOT/Binaries"
DEST="$BINARIES/truapi_provider.xcframework"

if [ ! -d "$PROVIDER_XCFRAMEWORK" ]; then
    echo "error: $PROVIDER_XCFRAMEWORK is missing." >&2
    echo "Run 'scripts/rebuild.sh' first; this script only copies." >&2
    exit 66
fi

mkdir -p "$BINARIES"

# rm -rf would take the source with it, and the next run reports it missing
# rather than what happened.
if [ "$(cd "$PROVIDER_XCFRAMEWORK" && pwd -P)" = "$(cd "$BINARIES" && pwd -P)/truapi_provider.xcframework" ]; then
    echo "error: PROVIDER_XCFRAMEWORK is the staging destination: $DEST" >&2
    echo "Point it at the framework built under target/." >&2
    exit 64
fi

rm -rf "$DEST"
# Copy to DEST by name, or the result lands under the source's name and the
# strip below finds nothing.
cp -R "$PROVIDER_XCFRAMEWORK" "$DEST"

find "$DEST" -path '*/Headers/module.modulemap' -delete

echo "XCFramework staged into $DEST"
