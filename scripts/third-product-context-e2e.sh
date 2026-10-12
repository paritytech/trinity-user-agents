#!/usr/bin/env bash
# A ring-VRF proof in a third product's context against a real signing-host
# CLI, driven through the `@parity/truapi` client. The end-to-end check for
# paritytech/platform-bugs#99.
#
# dim2next.paseo proves with peopl.paseo's key in dim2.paseo's context, which
# dim2.paseo's local product config grants, and in stash.paseo's, which grants
# nothing. The first is admitted and the second refused; before the fix both
# were refused. An admitted proof still answers `NotMember`, because the CLI
# account is not a person and membership is checked after the context.
#
# This builds `truapi-host` from the checkout, so it always tests the code in
# front of it rather than whatever binary is installed.
#
# The runner serves one product per host process, so each phase is its own
# `truapi-host` run. They share one `--base-path`, so the key the second phase
# proves with was registered by a process that has already exited.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fixtures="rust/crates/truapi-host-cli/js/fixtures/third-product-context"
script="rust/crates/truapi-host-cli/js/third-product-context-e2e.ts"
network="${E2E_NETWORK:-paseo-next-v2}"

# Kept between runs, for the reason `cross-product-ringvrf-e2e.sh` gives: the
# first run registers a lite username on a real chain, and this directory holds
# the only copy of the account that owns it.
#
# Point E2E_STATE_DIR at an existing base path to reuse its account, or set
# E2E_SESSION to choose a username base for a fresh identity.
state="${E2E_STATE_DIR:-$repo_root/target/e2e/third-product-context}"

# A base restores its newest local session; a full username selects it exactly.
# Creating an account requires at least six lowercase ASCII letters in the base.
session="${E2E_SESSION:-}"
session_arg=()
[ -n "$session" ] && session_arg=(--session "$session")
mkdir -p "$state"

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
      ${session_arg[@]+"${session_arg[@]}"} \
    --product-id "$product" \
    --product-config "$fixtures/peopl.paseo.json" \
    --product-config "$fixtures/dim2.paseo.json" \
    --product-config "$fixtures/stash.paseo.json" \
    --auto-accept \
    --script "$script"
}

run_phase register peopl.paseo
run_phase prove dim2next.paseo
