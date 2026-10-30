#!/usr/bin/env bash
# Statement-store renewal against a full period, on a real signing-host CLI and
# the People chain. The end-to-end check for paritytech/platform-bugs#88.
#
# A fresh lite identity allocates an allowance for product A, then product B,
# and fills the rest of the period with random accounts. One more
# `alloc-check --submit` then replaces A's slot, the oldest, so A holds no slot
# while B still holds its own. Neither product has been renewed before, so the
# pass takes them in ledger order: A finds the period full, and B is reached
# only by the scan that follows exhaustion. The pass must report A as exhausted
# and B as already allocated, in that order, and leave every slot as it was. A
# renewal that replaced a slot would take one of the random accounts'
# allowances.
#
# This builds `truapi-host` from the checkout, so it always tests the code in
# front of it rather than whatever binary is installed. The test needs a period
# with no other registrations, so each run provisions and attests a new lite
# identity in its state directory and uses up that identity's slots for the
# period: N + 3 real slot registrations, N being the identity's slots per
# period. A run takes about five minutes.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Homebrew LLVM on DYLD_LIBRARY_PATH makes rustc load a mismatched libLLVM.
unset DYLD_LIBRARY_PATH
# A mnemonic would make the signing host an ephemeral session without the
# renewal ledger this test relies on. Only `alloc-check` is given one.
unset HOST_CLI_SIGNER_MNEMONIC

script="rust/crates/truapi-host-cli/js/renewal-full-period-e2e.ts"
network="${E2E_NETWORK:-paseo-next-v2}"
product_a="${E2E_PRODUCT_A:-renewalrepro.dot}"
product_b="${E2E_PRODUCT_B:-renewalreprob.dot}"
# A lite identity proves only LitePeople. People scans as free but cannot be
# registered into, so only LitePeople decides whether the period is full.
collection="LitePeople"

state="${E2E_STATE_DIR:-$(mktemp -d)}"
mkdir -p "$state"
if [ -n "$(ls -A "$state")" ]; then
  echo "==> $state is not empty; pass an empty E2E_STATE_DIR" >&2
  exit 2
fi
cleanup() {
  local status=$?
  if [ "$status" -ne 0 ]; then
    echo "==> logs kept in $state" >&2
  elif [ -z "${E2E_STATE_DIR:-}" ]; then
    rm -rf "$state"
  fi
}
trap cleanup EXIT

echo "==> building truapi-host"
cargo build -q -p truapi-host-cli
host="target/debug/truapi-host"
signer=(--network "$network" --base-path "$state/signer")

name="renewal$(openssl rand -hex 4 | tr 0-9 a-j)"
echo "==> provisioning lite identity $name"
if ! "$host" signing-host "${signer[@]}" --session "$name" exec "/session $name" \
  > "$state/provision.log" 2>&1; then
  echo "==> provisioning $name failed" >&2
  tail -5 "$state/provision.log" >&2
  exit 1
fi
accounts="$(find "$state/signer" -name accounts.json | head -1)"
mnemonic="$(bun -e 'console.log(require(process.argv[1]).accounts[0].mnemonic)' "$accounts")"

check() {
  HOST_CLI_SIGNER_MNEMONIC="$mnemonic" "$host" alloc-check --network "$network" "$@"
}
# Buffered, so `grep -q` returning early cannot fail the check under pipefail.
check_finds() {
  local report
  report="$(check)"
  grep -q "$1" <<<"$report"
}
register_random() {
  check --target "0x$(openssl rand -hex 32)" --submit > /dev/null
}
# `seq=<n> since=<t> … account=<id>` per occupied slot, without the age, which
# moves with the clock.
slot_table() {
  check | grep -oE 'seq=[0-9]+ since=[0-9]+ .* account=0x[0-9a-f]+' |
    sed -E 's/ age=[0-9]+s [a-z ]+//'
}

for product in "$product_a" "$product_b"; do
  echo "==> allocating for $product"
  "$host" signing-host "${signer[@]}" --session "$name" --product-id "$product" \
    --auto-accept exec "/script $script" > "$state/allocate-$product.log" 2>&1
done

echo "==> filling the period"
while check_finds "^$collection: slot scan: free"; do
  register_random
done

echo "==> replacing the oldest slot, product A's, once its cooldown passes"
until check_finds 'would replace'; do sleep 15; done
register_random
before="$(slot_table)"

echo "==> renewing"
# Debug for the renewal pass, whose per-target lines show the order it ran in.
RUST_LOG="warn,truapi=info,truapi_host=info,truapi::runtime::statement_allowance::renewal=debug" \
  "$host" signing-host "${signer[@]}" --session "$name" exec '/renew' \
  > "$state/renew.log" 2>&1
after="$(slot_table)"

fail() {
  echo "==> $1" >&2
  cat "$state/renew.log" >&2
  exit 1
}
grep -q "label=\"product:$product_a\" reason=no free StatementStore slot" "$state/renew.log" ||
  fail "product A was not reported as exhausted"
grep -q '0 renewed · 1 fresh · 1 failed · 0 skipped' "$state/renew.log" ||
  fail "expected product B already allocated and product A exhausted"
line_of() {
  grep -n "$1" "$state/renew.log" | grep -F "label=\"product:$2\"" | head -1 | cut -d: -f1
}
a_line="$(line_of 'statement-store renewal failed' "$product_a")"
b_line="$(line_of 'statement-store allowance already fresh' "$product_b")"
[ -n "$a_line" ] && [ -n "$b_line" ] && [ "$b_line" -gt "$a_line" ] ||
  fail "product B was not reached by the scan after the period filled"
[ "$before" = "$after" ] || fail "renewal changed the slot table"

echo "==> renewal full period e2e passed"
