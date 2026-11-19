#!/usr/bin/env bash
# A ring-VRF proof in a third product's context against a real signing-host
# CLI, driven through the `@parity/truapi` client. The end-to-end check for
# paritytech/platform-bugs#99.
#
# dim2next.paseo proves with peopl.paseo's key in dim2.paseo's context, which
# dim2.paseo's local product config grants, and in stash.paseo's, which grants
# nothing. The first is admitted and the second refused; before the fix both
# were refused. An admitted proof still answers `NotMember`, because the test
# signer is not a person and membership is checked after the context.
#
# This builds `truapi-host` from the checkout, so it always tests the code in
# front of it rather than whatever binary is installed. Registering the key
# resolves a ring on the People chain; nothing is submitted.
#
# The runner serves one product per host process, so each phase is its own
# `truapi-host` run. They share one `--base-path`, so the key the second phase
# proves with was registered by a process that has already exited.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fixtures="rust/crates/truapi-host-cli/js/fixtures/ringvrf-context"
script="rust/crates/truapi-host-cli/js/cross-product-ringvrf-context-e2e.ts"
network="${E2E_NETWORK:-paseo-next-v2}"

# A mnemonic bypasses account auto-management, which would otherwise register a
# lite username on chain.
export HOST_CLI_SIGNER_MNEMONIC="${HOST_CLI_SIGNER_MNEMONIC:-bottom drive obey lake curtain smoke basket hold race lonely fit walk}"

state="${E2E_STATE_DIR:-$(mktemp -d)}"
[ -n "${E2E_STATE_DIR:-}" ] || trap 'rm -rf "$state"' EXIT

echo "==> building truapi-host"
cargo build -q -p truapi-host-cli
host="target/debug/truapi-host"

# Run one phase as one product. Every phase loads every config, so the grants
# are the same whichever product the host serves.
run_phase() {
  local phase="$1" product="$2"
  echo "==> $phase (as $product)"
  E2E_PHASE="$phase" "$host" signing-host \
    --network "$network" \
    --base-path "$state" \
    --product-id "$product" \
    --product-config "$fixtures/peopl.paseo.json" \
    --product-config "$fixtures/dim2.paseo.json" \
    --product-config "$fixtures/stash.paseo.json" \
    --auto-accept \
    --script "$script"
}

run_phase register peopl.paseo
run_phase prove dim2next.paseo

echo "==> cross-product ring-VRF context e2e passed"
