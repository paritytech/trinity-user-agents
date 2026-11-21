#!/usr/bin/env bash
# Drop the stale incremental caches in a cargo profile's output directory.
#
# Cargo starts a new incremental directory whenever a crate's build hash changes
# (a toolchain update, a feature change, a dependency bump) and never removes the
# old one, so an incremental profile grows by a full cache per change. For each
# crate this keeps the newest directory, plus any other used within the last
# <keep-days> days: one crate can have several caches in use at once, because
# different commands build it with different features (`make uniffi` and
# `make provider-swift`, or `cargo build` and `cargo test`).
#
# Usage: scripts/prune-incremental.sh <profile-dir> <keep-days>
#        (e.g. target/codegen 7)

set -euo pipefail

INCREMENTAL_DIR="$1/incremental"
KEEP_DAYS="$2"
[ -d "$INCREMENTAL_DIR" ] || exit 0

for crate in $(ls "$INCREMENTAL_DIR" | sed 's/-[^-]*$//' | sort -u); do
  ls -td "$INCREMENTAL_DIR/$crate"-* | tail -n +2 | while read -r candidate; do
    if [ -z "$(find "$candidate" -maxdepth 0 -mtime "-$KEEP_DAYS")" ]; then
      rm -rf "$candidate"
    fi
  done
done
