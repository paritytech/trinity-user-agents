#!/usr/bin/env bash
# Builds the reference funding provider (products/fundtest) and copies it into
# the app as a bundled product, so testnet builds run it with no dotNS or
# Bulletin lookup.
#
# The provider is built from products/fundtest in this checkout, so a build
# bundles only code that is in the branch being built. Without it (fundtest is
# not merged yet), nothing is bundled and any bundle left by an earlier run is
# removed.
#
# Output, ignored by git:
#   polkadot-app/BundledProducts.bundle/fundtest-provider.dot/
#     worker-manifest.json
#     worker/index.js
#     app/...
#
# The app builds without it and then has no bundled provider. Build-time
# FUNDTEST_* variables (FUNDTEST_CARD_ARRIVAL_MS, ...) are passed through to the
# provider's build.
#
# Usage: hosts/ios/Runscripts/bundle-funding-provider.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
IOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

PRODUCT_ID="fundtest-provider.dot"
DEST_ROOT="$IOS_DIR/polkadot-app/BundledProducts.bundle"
DEST="$DEST_ROOT/$PRODUCT_ID"
SOURCE="$REPO_ROOT/products/fundtest"

if [ ! -d "$SOURCE" ]; then
  rm -rf "$DEST"
  echo "bundle-funding-provider: products/fundtest is not in this checkout; nothing bundled"
  exit 0
fi

SHA="$(git -C "$REPO_ROOT" rev-parse HEAD)"
if [ -n "$(git -C "$REPO_ROOT" status --porcelain -- products/fundtest)" ]; then
  SHA="$SHA-dirty"
fi
echo "bundle-funding-provider: building $PRODUCT_ID from this checkout ($SHA)"

# The provider builds against this repository's @parity/truapi, which has to
# be generated and built first.
(
  cd "$REPO_ROOT"
  npm ci --no-audit --no-fund
  npm run build --prefix js/packages/truapi
)

(
  cd "$SOURCE"
  npm ci --no-audit --no-fund
  npm run build
)

for required in "$SOURCE/dist/worker/index.js" "$SOURCE/dist/app/index.html" "$SOURCE/worker-manifest.json"; do
  if [ ! -f "$required" ]; then
    echo "bundle-funding-provider: the build left no $required" >&2
    exit 1
  fi
done

# Staged beside the destination and swapped in whole, so an interrupted run
# never leaves half a provider in the app.
STAGING="$DEST_ROOT/.$PRODUCT_ID.staging"
rm -rf "$STAGING"
mkdir -p "$STAGING/worker"
cp "$SOURCE/worker-manifest.json" "$STAGING/worker-manifest.json"
cp "$SOURCE/dist/worker/index.js" "$STAGING/worker/index.js"
cp -R "$SOURCE/dist/app" "$STAGING/app"
echo "$SHA" > "$STAGING/SOURCE_COMMIT"

rm -rf "$DEST"
mv "$STAGING" "$DEST"

echo "bundle-funding-provider: $PRODUCT_ID bundled into $DEST"
