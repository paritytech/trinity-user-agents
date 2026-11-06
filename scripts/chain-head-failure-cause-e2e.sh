#!/usr/bin/env bash
# End-to-end check for paritytech/platform-bugs#126: a signing-host CLI, driven
# through `@parity/truapi`, loses its Bulletin connection under a chain-head
# storage call (a local proxy closes it, routed through a local copy of the
# paseo-next-v2 stack passed with `--stack`). Both `HostFailure` reasons the
# product sees must be `<method>: <cause>`.
# Read-only; nothing is written on chain.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

script="rust/crates/truapi-host-cli/js/chain-head-failure-cause-e2e.ts"
proxy_script="rust/crates/truapi-host-cli/js/chain-head-failure-cause-proxy.ts"
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

# The paseo-next-v2 stack this build ships, whose Bulletin the proxy forwards to.
descriptor="$("$host" stack verify rust/crates/truapi-host-cli/stacks/paseo-next-v2.json | jq .descriptor)"
bulletin_ws="$(jq -r '.chains[] | select(.role == "Bulletin") | .rpc[0]' <<<"$descriptor")"

echo "==> starting the Bulletin proxy"
bun "$proxy_script" "$bulletin_ws" "$state/proxy-port" &
proxy_pid=$!
for _ in $(seq 1 100); do
  [ -s "$state/proxy-port" ] && break
  kill -0 "$proxy_pid" 2>/dev/null || { echo "the proxy exited before listening" >&2; exit 1; }
  sleep 0.1
done
[ -s "$state/proxy-port" ] || { echo "the proxy did not start listening" >&2; exit 1; }
jq --arg ws "ws://127.0.0.1:$(cat "$state/proxy-port")" \
  '(.chains[] | select(.role == "Bulletin") | .rpc) = [$ws]' <<<"$descriptor" >"$state/stack.json"

echo "==> following Bulletin and cutting the connection under a storage call"
"$host" signing-host \
  --stack "$state/stack.json" \
  --base-path "$state/host" \
  --auto-accept \
  --script "$script"

echo "==> chain-head failure cause e2e passed"
