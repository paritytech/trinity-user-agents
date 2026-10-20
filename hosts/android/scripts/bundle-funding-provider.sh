#!/usr/bin/env bash
# Builds the reference funding provider (products/fundtest) and copies it into
# the app as a bundled product, so testnet builds run it with no dotNS or
# Bulletin lookup.
#
# A local opt-in: no build runs this, and every build completes and runs
# without it, shipping no bundled provider. The source is chosen explicitly:
#   FUNDTEST_DIR=<path>   a fundtest checkout on disk
#   FUNDTEST_REF=<ref>    a branch or commit of this repository, built in a
#                         detached worktree kept between runs (e.g.
#                         funding/reference-provider for the draft)
#   otherwise             products/fundtest in this checkout, once merged
# With none of these, it says so and changes nothing. `--clean` removes the
# bundle.
#
# Output, ignored by git and packaged only into the debug, nightly and
# safetynet builds:
#   feature/products/impl/bundled-assets/bundled-products/fundtest-provider.dot/
#     worker-manifest.json
#     worker/index.js
#     app/...
#
# The app builds without it and then has no bundled provider. Build-time
# FUNDTEST_* variables (FUNDTEST_CARD_ARRIVAL_MS, ...) are passed through to the
# provider's build.
#
# Usage: [FUNDTEST_DIR=… | FUNDTEST_REF=…] hosts/android/scripts/bundle-funding-provider.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ANDROID_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

PRODUCT_ID="fundtest-provider.dot"
DEST_ROOT="$ANDROID_DIR/feature/products/impl/bundled-assets/bundled-products"
DEST="$DEST_ROOT/$PRODUCT_ID"
WORK_DIR="${FUNDTEST_WORK_DIR:-${TMPDIR:-/tmp}/polkadot-app-bundled-products/source}"

if [ "${1:-}" = "--clean" ]; then
  rm -rf "$DEST"
  echo "bundle-funding-provider: removed $DEST"
  exit 0
fi

if [ -n "${FUNDTEST_DIR:-}" ]; then
  SOURCE="$(cd "$FUNDTEST_DIR" && pwd)"
  BUILD_ROOT="$(git -C "$SOURCE" rev-parse --show-toplevel)"
  SHA="$(git -C "$SOURCE" rev-parse HEAD)"
elif [ -n "${FUNDTEST_REF:-}" ]; then
  if git -C "$REPO_ROOT" rev-parse --verify -q "$FUNDTEST_REF^{commit}" >/dev/null; then
    SHA="$(git -C "$REPO_ROOT" rev-parse "$FUNDTEST_REF^{commit}")"
  elif git -C "$REPO_ROOT" rev-parse --verify -q "origin/$FUNDTEST_REF^{commit}" >/dev/null; then
    SHA="$(git -C "$REPO_ROOT" rev-parse "origin/$FUNDTEST_REF^{commit}")"
  else
    echo "bundle-funding-provider: $FUNDTEST_REF is neither a local nor an origin ref" >&2
    exit 1
  fi
  if [ -e "$WORK_DIR/.git" ]; then
    # Ignored files (node_modules, target, vendor) survive, which is the point.
    git -C "$WORK_DIR" checkout -q --force --detach "$SHA"
  else
    git -C "$REPO_ROOT" worktree prune
    mkdir -p "$(dirname "$WORK_DIR")"
    git -C "$REPO_ROOT" worktree add -q --detach "$WORK_DIR" "$SHA"
  fi
  BUILD_ROOT="$WORK_DIR"
  SOURCE="$WORK_DIR/products/fundtest"
elif [ -d "$REPO_ROOT/products/fundtest" ]; then
  BUILD_ROOT="$REPO_ROOT"
  SOURCE="$REPO_ROOT/products/fundtest"
  SHA="$(git -C "$REPO_ROOT" rev-parse HEAD)"
else
  echo "bundle-funding-provider: no fundtest source. Set FUNDTEST_DIR=<path> or FUNDTEST_REF=<ref>" \
    "(e.g. funding/reference-provider); products/fundtest is not in this checkout. Nothing changed."
  exit 0
fi

if [ ! -d "$SOURCE" ]; then
  echo "bundle-funding-provider: $SOURCE does not exist" >&2
  exit 1
fi
echo "bundle-funding-provider: building $PRODUCT_ID from $SOURCE ($SHA)"

# The provider builds against that tree's @parity/truapi, which has to be
# generated and built first.
(
  cd "$BUILD_ROOT"
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
# never leaves half a provider in the app. The staging name starts with a dot,
# which the app skips and aapt leaves out of the package.
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
