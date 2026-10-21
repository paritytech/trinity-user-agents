#!/bin/sh
# Copy the xcframework built by `make xcframework` into the package's gitignored
# Binaries/, then drop the per-slice modulemaps.
#
# Xcode's ProcessXCFramework step flattens every slice's Headers into one
# DerivedData include directory, so a slice-local module.modulemap collides with
# any other xcframework that ships one. truapiFFI resolves its module
# through the .systemLibrary SPM target instead.
#
# Requires `make xcframework` to have run first; this script builds nothing.
set -eu

PACKAGE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TRUAPI_ROOT="$(cd "$PACKAGE_ROOT/../.." && pwd)"
SOURCE="$TRUAPI_ROOT/target/truapi_server.xcframework"
DEST="$PACKAGE_ROOT/Binaries/truapi_server.xcframework"

if [ ! -d "$SOURCE" ]; then
    echo "error: $SOURCE is missing." >&2
    echo "Run 'make xcframework' at the repo root first; this script only copies." >&2
    exit 66
fi

# Package.swift links this copy only when the path exists while Xcode reads the
# manifest, and Xcode keeps that choice. If the path is missing, Xcode links
# the published release instead. Thus put the new copy beside the old one and
# swap them, so that the path does not disappear.
STAGING="$DEST.staging"
OLD="$DEST.old"
rm -rf "$STAGING" "$OLD"
mkdir -p "$PACKAGE_ROOT/Binaries"
cp -R "$SOURCE" "$STAGING"

# The slice set follows XCFRAMEWORK_TARGETS, so match on the layout rather than
# on fixed slice names.
find "$STAGING" -path '*/Headers/module.modulemap' -delete

if [ -d "$DEST" ]; then
    mv "$DEST" "$OLD"
fi
mv "$STAGING" "$DEST"
rm -rf "$OLD"

echo "XCFramework staged into $PACKAGE_ROOT/Binaries"
