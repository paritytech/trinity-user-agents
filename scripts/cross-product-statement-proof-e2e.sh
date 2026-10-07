#!/usr/bin/env bash
# Statement proofs signed with another product's account on a real signing-host
# CLI, counting how often the user is asked.
#
# `dim2next.paseo` signs statements as `dim2.paseo`, which the `context` grant
# in `dim2.paseo`'s local product config permits. The user must approve each
# account once, and not each statement. Run it on a build without that and the
# first phase reports five prompts and fails.
#
# Chain-free. The grant resolves from `--product-config` and the proof is signed
# locally, so nothing is submitted.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fixtures="rust/crates/truapi-host-cli/js/fixtures"
script="rust/crates/truapi-host-cli/js/cross-product-statement-proof-e2e.ts"
network="${E2E_NETWORK:-paseo-next-v2}"

# A mnemonic bypasses account auto-management, which would otherwise register a
# lite username on chain.
export HOST_CLI_SIGNER_MNEMONIC="${HOST_CLI_SIGNER_MNEMONIC:-bottom drive obey lake curtain smoke basket hold race lonely fit walk}"

state="${E2E_STATE_DIR:-$(mktemp -d)}"
trap 'rm -rf "$state"' EXIT
export TRUAPI_APPROVALS_LOG="$state/approvals.log"

echo "==> building truapi-host"
cargo build -q -p truapi-host-cli

echo "==> statement proofs (as dim2next.paseo)"
target/debug/truapi-host signing-host \
  --network "$network" \
  --base-path "$state" \
  --product-id dim2next.paseo \
  --product-config "$fixtures/dim2.paseo.trusts-dim2next.json" \
  --auto-accept \
  --script "$script"

echo "==> cross-product statement proof e2e passed"
