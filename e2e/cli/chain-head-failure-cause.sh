#!/usr/bin/env bash
# End-to-end check for paritytech/platform-bugs#126: a signing-host CLI, driven
# through `@parity/truapi`, loses its Bulletin connection under a chain-head
# storage call (a local proxy routed by `HOST_CLI_BULLETIN_WS` closes it). Both
# `HostFailure` reasons the product sees must be `<method>: <cause>`.
# Read-only; nothing is written on chain.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

script="rust/crates/truapi-host-cli/js/chain-head-failure-cause-e2e.ts"
proxy_script="rust/crates/truapi-host-cli/js/chain-head-failure-cause-proxy.ts"
# paseo-next-v2 Bulletin, as `network.rs` declares it.
bulletin_ws="wss://paseo-bulletin-next-rpc.polkadot.io"

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

echo "==> starting the Bulletin proxy"
bun "$proxy_script" "$bulletin_ws" "$state/proxy-port" &
proxy_pid=$!
for _ in $(seq 1 100); do
  [ -s "$state/proxy-port" ] && break
  kill -0 "$proxy_pid" 2>/dev/null || { echo "the proxy exited before listening" >&2; exit 1; }
  sleep 0.1
done
[ -s "$state/proxy-port" ] || { echo "the proxy did not start listening" >&2; exit 1; }
export HOST_CLI_BULLETIN_WS="ws://127.0.0.1:$(cat "$state/proxy-port")"

echo "==> following Bulletin and cutting the connection under a storage call"
"$host" signing-host \
  --network paseo-next-v2 \
  --base-path "$state/host" \
  --auto-accept \
  --script "$script"

echo "==> chain-head failure cause e2e passed"
