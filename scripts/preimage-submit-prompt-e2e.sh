#!/usr/bin/env bash
# `preimage.submit` under a `PreimageSubmit` grant against a real signing-host
# CLI, driven through the `@parity/truapi` client.
#
# Grant the capability once, then attempt three uploads without an interactive
# reviewer. `--auto-accept` must not grant bounded automatic-upload consent:
# the transcript must contain the permission prompt and three denied reviews.
#
# Build from the checkout and exercise the real signing host. No upload should
# reach Bulletin because explicit upload consent is absent.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

script="rust/crates/truapi-host-cli/js/preimage-submit-prompt-e2e.ts"
network="${E2E_NETWORK:-paseo-next-v2}"
product="${E2E_PRODUCT:-truapi-playground.dot}"

# A mnemonic bypasses account auto-management, which would otherwise register a
# lite username on chain.
export HOST_CLI_SIGNER_MNEMONIC="${HOST_CLI_SIGNER_MNEMONIC:-bottom drive obey lake curtain smoke basket hold race lonely fit walk}"

state="${E2E_STATE_DIR:-$(mktemp -d)}"
[ -n "${E2E_STATE_DIR:-}" ] || trap 'rm -rf "$state"' EXIT

# A fresh transcript, so only this run's prompts are counted.
export TRUAPI_APPROVALS_LOG="$state/approvals.log"
rm -f "$TRUAPI_APPROVALS_LOG"

echo "==> building truapi-host"
cargo build -q -p truapi-host-cli
host="target/debug/truapi-host"

echo "==> checking explicit upload consent as $product"
"$host" signing-host \
  --network "$network" \
  --base-path "$state" \
  --product-id "$product" \
  --auto-accept \
  --script "$script" </dev/null

echo "==> preimage submit prompt e2e passed"
