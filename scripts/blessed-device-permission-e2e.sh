#!/usr/bin/env bash
# A product's notification against a real signing-host CLI, driven through the
# `@parity/truapi` client. The end-to-end check for paritytech/platform-bugs#82.
#
# Each run requests the Notifications permission and sends a notification,
# first as a blessed product and then as an ordinary one. The blessed run must
# raise no device permission prompt; the ordinary run raises exactly one.
#
# This builds `truapi-host` from the checkout, so it always tests the code in
# front of it rather than whatever binary is installed.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

script="rust/crates/truapi-host-cli/js/blessed-device-permission-e2e.ts"
network="${E2E_NETWORK:-paseo-next-v2}"
blessed_product="${E2E_BLESSED_PRODUCT:-peopl.paseo}"
ordinary_product="${E2E_PRODUCT:-truapi-playground.dot}"

# A mnemonic bypasses account auto-management, which would otherwise register a
# lite username on chain.
export HOST_CLI_SIGNER_MNEMONIC="${HOST_CLI_SIGNER_MNEMONIC:-bottom drive obey lake curtain smoke basket hold race lonely fit walk}"

state="${E2E_STATE_DIR:-$(mktemp -d)}"
[ -n "${E2E_STATE_DIR:-}" ] || trap 'rm -rf "$state"' EXIT
export TRUAPI_APPROVALS_LOG="$state/approvals.log"

echo "==> building truapi-host"
cargo build -q -p truapi-host-cli
host="target/debug/truapi-host"

notify_as() {
  local product="$1" blessed="$2"
  # A fresh transcript, so only this run's prompts are counted.
  rm -f "$TRUAPI_APPROVALS_LOG"
  echo "==> sending a notification as $product"
  E2E_BLESSED="$blessed" "$host" signing-host \
    --network "$network" \
    --base-path "$state" \
    --product-id "$product" \
    --auto-accept \
    --script "$script"
}

notify_as "$blessed_product" 1
notify_as "$ordinary_product" 0

echo "==> blessed device permission e2e passed"
