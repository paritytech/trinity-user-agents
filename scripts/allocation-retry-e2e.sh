#!/usr/bin/env bash
# A resource allocation retried after the product got no answer to the first
# one, against a pairing-host CLI paired with a real signing-host CLI, driven
# through the `@parity/truapi` client.
#
# The signing host stands in for the phone and approves one allocation only
# after the product's 120 s request deadline, so the product gives up on it.
# The product's retry must reuse the person's slot rather than register
# another. See allocation-retry-e2e.ts.
#
# This builds `truapi-host` from the checkout, so it always tests the code in
# front of it rather than whatever binary is installed. The allocations are
# real statement-store slot registrations on the People chain, so the signing
# host must be a person: it uses its default base path, which holds its
# attested account, as `scripts/battery.sh` does. A run takes about three
# minutes and registers one slot from the person's budget for the period.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Homebrew LLVM on DYLD_LIBRARY_PATH makes rustc load a mismatched libLLVM.
unset DYLD_LIBRARY_PATH
# This script sets it for the signing host alone; an inherited value would
# reach the pairing host too.
unset TRUAPI_ALLOCATION_APPROVAL_DELAYS_MS

script="rust/crates/truapi-host-cli/js/allocation-retry-e2e.ts"
network="${E2E_NETWORK:-paseo-next-v2}"
product="${E2E_PRODUCT:-truapi-playground.dot}"
pairing_timeout="${E2E_PAIRING_TIMEOUT:-120}"

# A reused state directory would restore a session with a signing host that
# has exited, so each run starts from an empty one.
state="${E2E_STATE_DIR:-$(mktemp -d)}"
mkdir -p "$state"
if [ -n "$(ls -A "$state")" ]; then
  echo "==> $state is not empty; pass an empty E2E_STATE_DIR" >&2
  exit 2
fi
log="$state/pairing-host.log"
export E2E_SIGNER_LOG="$state/signing-host.log"

echo "==> building truapi-host"
cargo build -q -p truapi-host-cli
host="target/debug/truapi-host"

host_pid="" signer_pid=""
cleanup() {
  local status=$?
  for pid in $host_pid $signer_pid; do
    kill "$pid" 2>/dev/null || true
  done
  if [ "$status" -ne 0 ]; then
    echo "==> host logs kept in $state" >&2
  elif [ -z "${E2E_STATE_DIR:-}" ]; then
    rm -rf "$state"
  fi
}
trap cleanup EXIT

echo "==> starting the product on a pairing host"
"$host" pairing-host \
  --network "$network" \
  --base-path "$state/pairing-host" \
  --product-id "$product" \
  --script "$script" \
  --auto-accept > >(tee "$log") 2>&1 &
host_pid=$!

deeplink="" waited=0
while [ "$waited" -lt "$pairing_timeout" ]; do
  deeplink="$(grep -o 'polkadotapp://pair?[^[:space:]]*' "$log" | head -1 || true)"
  [ -n "$deeplink" ] && break
  kill -0 "$host_pid" 2>/dev/null || break
  sleep 1
  waited=$((waited + 1))
done
if [ -z "$deeplink" ]; then
  echo "==> no pairing link after ${pairing_timeout}s" >&2
  exit 1
fi

# One delay per allocation approval, in the order the product's steps ask:
# baseline, late, retry. Debug logging on the responder records whether each
# allocation registered a slot or reused the person's one.
echo "==> answering the pairing link with a signing host that approves late"
TRUAPI_ALLOCATION_APPROVAL_DELAYS_MS="0,130000,0" \
RUST_LOG="${RUST_LOG:-warn,truapi=info},truapi::runtime::signing_host::allowance=debug" \
  "$host" signing-host \
  --network "$network" \
  --auto-accept \
  --product-id "$product" \
  --frame-listen 127.0.0.1:0 \
  exec "/pair $deeplink" > >(tee "$E2E_SIGNER_LOG" | sed 's/^/[signer] /') 2>&1 &
signer_pid=$!

wait "$host_pid"
host_pid=""
echo "==> allocation retry e2e passed"
