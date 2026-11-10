#!/usr/bin/env bash
# `preimage.submit` under a `PreimageSubmit` grant against a real signing-host
# CLI, driven through the `@parity/truapi` client. The end-to-end check for
# paritytech/platform-bugs#54.
#
# The product is granted `PreimageSubmit` once and uploads three preimages to
# Bulletin. The host's approvals transcript must hold that one permission prompt
# and no per-upload confirmation; before the fix it held one per upload.
#
# This builds `truapi-host` from the checkout, so it always tests the code in
# front of it rather than whatever binary is installed. The uploads are real
# Bulletin submissions.
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

echo "==> uploading as $product"
"$host" signing-host \
  --network "$network" \
  --base-path "$state" \
  --product-id "$product" \
  --auto-accept \
  --script "$script"

echo "==> preimage submit prompt e2e passed"
