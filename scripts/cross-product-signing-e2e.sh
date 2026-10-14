#!/usr/bin/env bash
# Cross-product account signing against a real signing-host CLI, driven through
# the `@parity/truapi` client.
#
# One product signs with another product's account; only the manifest grant in
# `peopl.paseo`'s local product config permits that. The sibling of
# `cross-product-ringvrf-e2e.sh`, for the same `context` scope on the signing
# methods rather than on a ring-VRF key.
#
# Chain-free. Every manifest a phase resolves is seeded by `--product-config`,
# including the owner's in the untrusted phase, which is the document that
# refuses it.
#
# The runner serves one product per host process, so each phase is its own
# `truapi-host` run. They share one `--base-path`, which is what makes the
# signature genuinely cross-product: the account the granted phases name was
# parked by a process that has already exited.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fixtures="rust/crates/truapi-host-cli/js/fixtures"
script="rust/crates/truapi-host-cli/js/cross-product-signing-e2e.ts"
network="${E2E_NETWORK:-paseo-next-v2}"

# A mnemonic bypasses account auto-management, which would otherwise register a
# lite username on chain and fail the moment the prefix is already taken. The
# grant path this exercises does not care which root signs, only that both
# phases share one.
export HOST_CLI_SIGNER_MNEMONIC="${HOST_CLI_SIGNER_MNEMONIC:-bottom drive obey lake curtain smoke basket hold race lonely fit walk}"

state="${E2E_STATE_DIR:-$(mktemp -d)}"
trap 'rm -rf "$state"' EXIT

echo "==> building truapi-host"
cargo build -q -p truapi-host-cli
host="target/debug/truapi-host"

# Run one phase as one product. A phase that exits non-zero fails the script,
# including the phase whose assertion is that a signature was refused: the
# script distinguishes a refusal it expected from a host that fell over.
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

run_phase sign-owner peopl.paseo
run_phase sign-granted dim2.paseo
run_phase sign-untrusted stash.paseo
# Last, so the refusal above cannot have been the grant expiring or being cleared.
run_phase sign-again dim2.paseo

echo "==> cross-product signing e2e passed"
