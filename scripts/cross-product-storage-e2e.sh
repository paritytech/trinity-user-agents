#!/usr/bin/env bash
# Cross-product storage against a real signing-host CLI, driven through the
# `@parity/truapi` client.
#
# One product writes to its own storage and another reads it; only the manifest
# grant in `peopl.paseo`'s local product config permits that. The granted read
# touches no chain: the host resolves it from `--product-config`, which seeds the
# manifest cache. The `read-missing` phase has nothing seeded, so its cache miss
# does go to dotNS on Asset Hub before refusing.
#
# The runner serves one product per host process, so each phase is its own
# `truapi-host` run. They share one `--base-path`, which is what makes the read
# genuinely cross-product rather than cross-connection: the value the read
# phases return was written by a process that has already exited.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fixtures="rust/crates/truapi-host-cli/js/fixtures"
script="rust/crates/truapi-host-cli/js/cross-product-storage-e2e.ts"
network="${E2E_NETWORK:-paseo-next-v2}"

state="$(mktemp -d)"
trap 'rm -rf "$state"' EXIT

echo "==> building truapi-host"
cargo build -q -p truapi-host-cli
host="target/debug/truapi-host"

# Run one phase as one product. A phase that exits non-zero fails the script,
# including the phases whose assertion is that a read was refused: the script
# distinguishes a refusal it expected from a host that fell over.
run_phase() {
  local phase="$1" product="$2"
  echo "==> $phase (as $product)"
  E2E_PHASE="$phase" "$host" signing-host \
    --network "$network" \
    --base-path "$state" \
    --product-id "$product" \
    --product-config "$fixtures/peopl.paseo.json" \
    --product-config "$fixtures/dim2.paseo.json" \
    --auto-accept \
    --script "$script"
}

run_phase write peopl.paseo
run_phase read dim2.paseo
run_phase read-untrusted stash.paseo
run_phase read-missing dim2.paseo
# Last, so a refusal above cannot have been the value expiring or being cleared.
run_phase read-again dim2.paseo

echo "==> cross-product storage e2e passed"
