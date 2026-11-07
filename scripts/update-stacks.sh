#!/usr/bin/env bash
# Refresh each stack descriptor truapi-host bundles in rust/crates/truapi-host-cli/stacks from its link,
# and the copy a host app bundles of the same stack.
# Fails when any stack was not refreshed; how the CLI uses the copies is in that crate's SPEC.md §14.1.
# Usage: TRUAPI_HOST=<truapi-host binary, default `truapi-host` on PATH> bash scripts/update-stacks.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
STACKS_DIR="$REPO_DIR/rust/crates/truapi-host-cli/stacks"
HOST="${TRUAPI_HOST:-truapi-host}"

# Where each host app keeps its copy of a stack, `{name}` standing for the stack's name.
HOST_COPIES=(
    "$REPO_DIR/hosts/ios/polkadot-app/Modules/Products/TrUAPI/Stacks/trinity-stack-{name}.json"
    "$REPO_DIR/hosts/android/feature/products/impl/src/main/assets/stacks/{name}.json"
)

command -v "$HOST" >/dev/null || {
    echo "error: $HOST is required but not on PATH; set TRUAPI_HOST" >&2
    exit 1
}

# A released truapi-host older than the flag rewrites every accepted copy.
skip_flag=()
if "$HOST" stack bundle --help | grep -q -- --skip-same-projection; then
    skip_flag=(--skip-same-projection)
fi

failed=()
for link_file in "$STACKS_DIR"/*.link; do
    name="$(basename "$link_file" .link)"
    echo "=== $name ==="
    if ! "$HOST" stack bundle "$(tr -d '[:space:]' <"$link_file")" \
        --out "$STACKS_DIR/$name.json" ${skip_flag[@]+"${skip_flag[@]}"}; then
        failed+=("$name")
        continue
    fi
    for pattern in "${HOST_COPIES[@]}"; do
        copy="${pattern//"{name}"/$name}"
        if [ -f "$copy" ]; then
            cp "$STACKS_DIR/$name.json" "$copy"
        fi
    done
done

if [ "${#failed[@]}" -gt 0 ]; then
    echo "error: not refreshed: ${failed[*]}" >&2
    exit 1
fi
