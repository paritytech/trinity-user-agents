#!/usr/bin/env bash
# Write the changeset of a bundled-refresh PR to `.changeset/bundled-refresh-<id>.md`: a patch for
# @parity/truapi when a bundled stack descriptor or chain spec changed (truapi-host compiles both
# in), and for @parity/truapi-provider when its chain specs did. Writes nothing when neither changed.
# Usage: bash scripts/write-refresh-changeset.sh <id>

set -euo pipefail

id="${1:?usage: write-refresh-changeset.sh <id>}"
cd "$(dirname "$0")/.."
stacks="$(git diff --name-only -- 'stacks/*.json')"
specs_changed=false
git diff --quiet -- rust/crates/truapi-provider/networks || specs_changed=true

if [ -z "$stacks" ] && [ "$specs_changed" = false ]; then
    echo "nothing refreshed; no changeset written"
    exit 0
fi

summary=()
if [ -n "$stacks" ]; then
    refreshed=()
    for file in $stacks; do
        sequence="$(jq -r .payload "$file" | base64 -d | jq -r .sequence)"
        refreshed+=("$(basename "$file" .json) (sequence $sequence)")
    done
    joined="$(printf '%s, ' "${refreshed[@]}")"
    summary+=("truapi-host ships refreshed stack descriptors for ${joined%, }.")
fi
if [ "$specs_changed" = true ]; then
    summary+=("truapi-provider and truapi-host ship refreshed light-client chain specs.")
fi

out=".changeset/bundled-refresh-$id.md"
{
    echo "---"
    echo '"@parity/truapi": patch'
    if [ "$specs_changed" = true ]; then echo '"@parity/truapi-provider": patch'; fi
    echo "---"
    echo
    echo "${summary[*]}"
} >"$out"
echo "wrote $out"
