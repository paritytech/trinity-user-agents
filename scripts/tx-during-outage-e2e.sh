#!/usr/bin/env bash
# End-to-end check that a transaction requested while the chain connection is
# down is built once the connection returns. A signing-host CLI, driven through
# `@parity/truapi`, builds one transaction on People, then a local proxy drops
# every connection and refuses reconnects for a few seconds, the way a phone
# loses its network when the app is backgrounded. Transactions requested in that
# window must still be built.
# Read-only; nothing is submitted.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

script="rust/crates/truapi-host-cli/js/tx-during-outage-e2e.ts"
proxy_script="rust/crates/truapi-host-cli/js/tx-during-outage-proxy.ts"
# A mnemonic bypasses account auto-management, which would otherwise register a
# lite username on chain.
export HOST_CLI_SIGNER_MNEMONIC="${HOST_CLI_SIGNER_MNEMONIC:-bottom drive obey lake curtain smoke basket hold race lonely fit walk}"

state="$(mktemp -d)"
proxy_pid=""
cleanup() {
  if [ -n "$proxy_pid" ]; then
    kill "$proxy_pid" 2>/dev/null || true
    wait "$proxy_pid" 2>/dev/null || true
  fi
  rm -rf "$state"
}
trap cleanup EXIT

echo "==> building truapi-host"
cargo build -q -p truapi-host-cli
host="target/debug/truapi-host"

# The paseo-next-v2 stack this build ships, whose People chain the proxy forwards to.
descriptor="$("$host" stack verify stacks/paseo-next-v2.json | jq .descriptor)"
people_ws="$(jq -r '.chains[] | select(.role == "People") | .rpc[0]' <<<"$descriptor")"

# Matches TRIGGER_KEY in the product script.
trigger_key="0x$(printf '7a%.0s' $(seq 1 32))"

echo "==> starting the People proxy"
bun "$proxy_script" "$people_ws" "$state/proxy-port" "${trigger_key}" "${OUTAGE_MS:-4000}" "${OUTAGE_MODE:-close}" &
proxy_pid=$!
for _ in $(seq 1 100); do
  [ -s "$state/proxy-port" ] && break
  kill -0 "$proxy_pid" 2>/dev/null || { echo "the proxy exited before listening" >&2; exit 1; }
  sleep 0.1
done
[ -s "$state/proxy-port" ] || { echo "the proxy did not start listening" >&2; exit 1; }
jq --arg ws "ws://127.0.0.1:$(cat "$state/proxy-port")" \
  '(.chains[] | select(.role == "People") | .rpc) = [$ws]' <<<"$descriptor" >"$state/stack.json"

echo "==> requesting transactions while the connection is down"
"$host" signing-host \
  --stack "$state/stack.json" \
  --base-path "$state/host" \
  --auto-accept \
  --script "$script" 2>&1 | tee "$state/host.log"

# A run where the cut never landed under a transaction proves nothing.
[ -f "$state/proxy-port.cut" ] || { echo "the proxy never cut the connection" >&2; exit 1; }
grep -q 'create_transaction.*retrying while the chain recovers' "$state/host.log" || {
  echo "no transaction had to wait out the outage" >&2
  exit 1
}

echo "==> transaction during outage e2e passed"
