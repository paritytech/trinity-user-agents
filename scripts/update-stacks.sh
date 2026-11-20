#!/usr/bin/env bash
# Write stacks/<name>.link and refresh stacks/<name>.json, the copies host builds bundle, for each
# known stack in rust/crates/trinity-stack/known-stacks.json whose link carries a key, skipping
# unsigned ones since host builds bundle only signed copies, and delete the copies of any other stack.
# Fails when any stack was not refreshed; how the CLI uses the copies is in
# rust/crates/truapi-host-cli/SPEC.md §14.1.
# Usage: TRUAPI_HOST=<truapi-host binary, default `truapi-host` on PATH> bash scripts/update-stacks.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
STACKS_DIR="$REPO_DIR/stacks"
KNOWN_STACKS="$REPO_DIR/rust/crates/trinity-stack/known-stacks.json"
HOST="${TRUAPI_HOST:-truapi-host}"

for tool in "$HOST" jq; do
    command -v "$tool" >/dev/null || {
        echo "error: $tool is required but not on PATH" >&2
        exit 1
    }
done

# A released truapi-host older than the flag rewrites every accepted copy.
skip_flag=()
if "$HOST" stack bundle --help | grep -q -- --skip-same-projection; then
    skip_flag=(--skip-same-projection)
fi

signed_stacks="$(jq -r 'to_entries[] | select(.value | test("[?&]key=")) | "\(.key)\t\(.value)"' "$KNOWN_STACKS")"
for link_file in "$STACKS_DIR"/*.link; do
    name="$(basename "$link_file" .link)"
    if ! cut -f1 <<<"$signed_stacks" | grep -qxF "$name"; then
        rm -f "$link_file" "$STACKS_DIR/$name.json"
    fi
done

failed=()
while IFS=$'\t' read -r name link; do
    echo "=== $name ==="
    printf '%s\n' "$link" >"$STACKS_DIR/$name.link"
    if ! "$HOST" stack bundle "$link" \
        --out "$STACKS_DIR/$name.json" ${skip_flag[@]+"${skip_flag[@]}"}; then
        failed+=("$name")
    fi
done <<<"$signed_stacks"

if [ "${#failed[@]}" -gt 0 ]; then
    echo "error: not refreshed: ${failed[*]}" >&2
    exit 1
fi
