#!/usr/bin/env bash
# Refresh each stack descriptor in stacks/, the copies host builds bundle, from its link, and the
# light specs of each stack with a stacks/light-specs/<name>.json, which truapi-host ships so its
# light client starts without the spec servers.
# Fails when any stack was not refreshed; how the CLI uses the copies is in that crate's SPEC.md §14.1.
# Usage: TRUAPI_HOST=<truapi-host binary, default `truapi-host` on PATH> bash scripts/update-stacks.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
STACKS_DIR="$(dirname "$SCRIPT_DIR")/stacks"
HOST="${TRUAPI_HOST:-truapi-host}"

command -v "$HOST" >/dev/null || {
    echo "error: $HOST is required but not on PATH; set TRUAPI_HOST" >&2
    exit 1
}

bundle_help="$("$HOST" stack bundle --help)"

# A released truapi-host older than the flag rewrites every accepted copy.
skip_flag=()
if grep -q -- --skip-same-projection <<<"$bundle_help"; then
    skip_flag=(--skip-same-projection)
fi
has_light_specs_flag=false
if grep -q -- --light-specs <<<"$bundle_help"; then
    has_light_specs_flag=true
fi

failed=()
for link_file in "$STACKS_DIR"/*.link; do
    name="$(basename "$link_file" .link)"
    echo "=== $name ==="
    light_specs_flag=()
    light_specs_file="$STACKS_DIR/light-specs/$name.json"
    if [ -f "$light_specs_file" ]; then
        # Refreshing the copy without its light specs would ship specs it no longer names.
        if [ "$has_light_specs_flag" = false ]; then
            echo "error: $HOST has no stack bundle --light-specs, which $name needs" >&2
            failed+=("$name")
            continue
        fi
        light_specs_flag=(--light-specs "$light_specs_file")
    fi
    if ! "$HOST" stack bundle "$(tr -d '[:space:]' <"$link_file")" \
        --out "$STACKS_DIR/$name.json" ${skip_flag[@]+"${skip_flag[@]}"} \
        ${light_specs_flag[@]+"${light_specs_flag[@]}"}; then
        failed+=("$name")
    fi
done

if [ "${#failed[@]}" -gt 0 ]; then
    echo "error: not refreshed: ${failed[*]}" >&2
    exit 1
fi
