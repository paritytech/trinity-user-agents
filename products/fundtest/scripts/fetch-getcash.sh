#!/usr/bin/env bash
# Check out paritytech/getcash-community at the pinned commit into vendor/, and
# generate its polkadot-api descriptors there. The provider imports getcash's
# packages from that checkout instead of copying them: getcash is
# GPL-3.0-or-later and this repository is MIT, so none of its source is
# committed here.
#
# Run from products/fundtest after `npm install` (npm runs it as `postinstall`).

set -euo pipefail

GETCASH_REPO="${GETCASH_REPO:-https://github.com/paritytech/getcash-community.git}"
GETCASH_COMMIT="ccbe32e03a9dd5e37646bdd40a287615e498a0ee"
DEST="vendor/getcash-community"

if [ ! -d "$DEST/.git" ] || [ "$(git -C "$DEST" rev-parse HEAD)" != "$GETCASH_COMMIT" ]; then
  rm -rf "$DEST"
  mkdir -p "$DEST"
  git -C "$DEST" init -q
  git -C "$DEST" remote add origin "$GETCASH_REPO"
  git -C "$DEST" fetch -q --depth 1 origin "$GETCASH_COMMIT"
  git -C "$DEST" checkout -q --detach FETCH_HEAD
  echo "getcash-community checked out at $GETCASH_COMMIT"
fi

# getcash's root tsconfig extends the one `nuxt prepare` writes, and the
# bundler reads it for every getcash source it transforms. Its packages compile
# against tsconfig.base.json, so the stand-in points there. `.nuxt/` is ignored
# by getcash, so the checkout stays clean.
if [ ! -f "$DEST/.nuxt/tsconfig.json" ]; then
  mkdir -p "$DEST/.nuxt"
  echo '{ "extends": "../tsconfig.base.json" }' > "$DEST/.nuxt/tsconfig.json"
fi

# The descriptors are generated from the metadata getcash commits, with no
# network access, in vendor/papi rather than inside the checkout, whose
# tsconfig needs a Nuxt build. `noDescriptorsPackage` keeps papi from running a
# package manager: they are resolved through the aliases in tsconfig.json and
# the Vite configs.
PAPI="$(pwd)/node_modules/.bin/papi"
if [ ! -f vendor/papi/descriptors/dist/index.js ]; then
  mkdir -p vendor/papi
  node -e '
    const fs = require("node:fs");
    const source = "vendor/getcash-community/.papi/";
    const config = JSON.parse(fs.readFileSync(source + "polkadot-api.json", "utf8"));
    for (const entry of Object.values(config.entries)) {
      entry.metadata = "../getcash-community/" + entry.metadata;
    }
    config.descriptorPath = "descriptors";
    config.options = { ...config.options, noDescriptorsPackage: true };
    fs.writeFileSync("vendor/papi/polkadot-api.json", JSON.stringify(config, null, 2));
  '
  ( cd vendor/papi && "$PAPI" generate --config polkadot-api.json )
fi
