#!/usr/bin/env bash
# Builds the reference funding provider (products/fundtest) and copies it into
# the app as a bundled product, so testnet builds run it with no dotNS or
# Bulletin lookup.
#
# The provider is built from what is committed on FUNDTEST_REF (default
# funding/reference-provider), in a detached worktree of this repository kept
# between runs so later builds reuse its node_modules and cargo target.
#
# Output, ignored by git:
#   polkadot-app/BundledProducts.bundle/fundtest-provider.dot/
#     worker-manifest.json
#     worker/index.js
#     app/...
#
# The app builds without it and then has no bundled provider. Build-time
# FUNDTEST_* variables (FUNDTEST_FAUCET_SEED, ...) are passed through to the
# provider's build.
#
# Usage: hosts/ios/Runscripts/bundle-funding-provider.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
IOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

REF="${FUNDTEST_REF:-funding/reference-provider}"
PRODUCT_ID="fundtest-provider.dot"
WORK_DIR="${FUNDTEST_WORK_DIR:-${TMPDIR:-/tmp}/polkadot-app-bundled-products/source}"
DEST_ROOT="$IOS_DIR/polkadot-app/BundledProducts.bundle"
DEST="$DEST_ROOT/$PRODUCT_ID"

if git -C "$REPO_ROOT" rev-parse --verify -q "$REF^{commit}" >/dev/null; then
  SHA="$(git -C "$REPO_ROOT" rev-parse "$REF^{commit}")"
elif git -C "$REPO_ROOT" rev-parse --verify -q "origin/$REF^{commit}" >/dev/null; then
  SHA="$(git -C "$REPO_ROOT" rev-parse "origin/$REF^{commit}")"
else
  echo "bundle-funding-provider: $REF is neither a local nor an origin branch" >&2
  exit 1
fi

echo "bundle-funding-provider: building $PRODUCT_ID from $REF ($SHA)"

if [ -e "$WORK_DIR/.git" ]; then
  # Ignored files (node_modules, target, vendor) survive, which is the point.
  git -C "$WORK_DIR" checkout -q --force --detach "$SHA"
else
  git -C "$REPO_ROOT" worktree prune
  mkdir -p "$(dirname "$WORK_DIR")"
  git -C "$REPO_ROOT" worktree add -q --detach "$WORK_DIR" "$SHA"
fi

# The provider builds against this repository's @parity/truapi, which has to
# be generated and built first.
(
  cd "$WORK_DIR"
  npm ci --no-audit --no-fund
  npm run build --prefix js/packages/truapi
)

(
  cd "$WORK_DIR/products/fundtest"
  npm ci --no-audit --no-fund
  npm run build
)

SOURCE="$WORK_DIR/products/fundtest"
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
