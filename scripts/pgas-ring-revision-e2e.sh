#!/usr/bin/env bash
# End-to-end check for paritytech/platform-bugs#153: a signing-host CLI asks for
# a PGAS allowance while Asset Hub appears to have skipped the claim's ring
# revision (a local proxy routed by `HOST_CLI_ASSET_HUB_WS` serves
# `MembersSubscriber.RingRoots` as pruned until the host reads it again for a
# rebuilt proof). The host has to rebuild the proof, submit it, and still answer
# `Allocated`.
#
# Submits one real claim on paseo-next-v2 and spends one of the day's slots.
# Signs as `HOST_CLI_SIGNER_MNEMONIC` when set, otherwise as the CLI-managed
# identity under `TRUAPI_HOST_BASE_PATH` (or the CLI default). Either has to be
# a person in a People ring already.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

script="rust/crates/truapi-host-cli/js/scripts/smart-contract-allowance-smoke.ts"
proxy_script="rust/crates/truapi-host-cli/js/pgas-ring-revision-proxy.ts"
# paseo-next-v2 Asset Hub, as `network.rs` declares it.
asset_hub_ws="wss://paseo-asset-hub-next-rpc.polkadot.io"

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

echo "==> starting the Asset Hub proxy"
bun "$proxy_script" "$asset_hub_ws" "$state/proxy-port" "$state/events" &
proxy_pid=$!
for _ in $(seq 1 100); do
  [ -s "$state/proxy-port" ] && break
  kill -0 "$proxy_pid" 2>/dev/null || { echo "the proxy exited before listening" >&2; exit 1; }
  sleep 0.1
done
[ -s "$state/proxy-port" ] || { echo "the proxy did not start listening" >&2; exit 1; }
export HOST_CLI_ASSET_HUB_WS="ws://127.0.0.1:$(cat "$state/proxy-port")"

echo "==> requesting a PGAS allowance while Asset Hub appears to skip the ring revision"
"$host" signing-host \
  --network paseo-next-v2 \
  --auto-accept \
  --script "$script" 2>&1 | tee "$state/host.log"

events="$(tr '\n' ' ' < "$state/events" 2>/dev/null || true)"
if [ "$events" != "pruned rebuilt submitted " ]; then
  echo "expected the proxy to see a pruned read, a rebuilt read and a submission; saw: ${events:-nothing}" >&2
  exit 1
fi
if ! grep -q "rebuilding the PGAS proof" "$state/host.log"; then
  echo "the host never logged rebuilding its PGAS proof" >&2
  exit 1
fi

echo "==> PGAS ring revision e2e passed"
