#!/usr/bin/env bash
# Refresh the bundled smoldot chain specs in rust/crates/truapi-provider/networks
# from their live chains.
#
# Each spec's genesis.stateRootHash is set from the chain's block 0, so the spec keeps matching the
# chain's genesis after a wipe; a stale genesis stops smoldot from syncing. Relay specs additionally
# get a fresh lightSyncState checkpoint, which reduces smoldot warp-sync time from ~12s to ~1-3s.
#
# The genesis and the checkpoint are what a light client trusts in place of syncing from block 0, so
# neither is taken on a single endpoint's word. Every reachable endpoint must serve the same genesis
# state root, and an endpoint other than the one that served the checkpoint must agree on the block
# it pins. A network with only one endpoint cannot be corroborated and says so.
#
# These specs are compiled into the @parity/truapi-provider wasm via include_str!, so a refresh only
# reaches consumers (e.g. dotli) after a new provider version is published and they bump to it.
#
# Usage: bash scripts/update-chain-specs.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SPECS_DIR="$PROJECT_DIR/rust/crates/truapi-provider/networks"

# Each refresh_spec call is `|| true` so one network's outage cannot block the
# others, and that disables `set -e` for the whole function body. Every failure
# inside it therefore has to be checked by hand, starting with the tools it needs:
# without this, a missing `node` reports every spec as refreshed while writing
# nothing.
for tool in node jq curl python3; do
    command -v "$tool" >/dev/null || {
        echo "error: $tool is required but not on PATH" >&2
        exit 1
    }
done

# Timeout (seconds) for all curl calls.
TIMEOUT=30

# Set when a spec could not be trusted: endpoints disagreed about its genesis or checkpoint, or no
# second endpoint could corroborate the checkpoint at all. Either way the spec is left unchanged and
# the run fails, so a trust anchor is never taken from one endpoint on its own.
INTEGRITY_FAILURES=0

# Health-check the candidate bootNodes (env var BOOTNODES) and keep only the reachable ones.
# Set env var SKIP_BOOTNODE_CHECK=true to leave them unchanged.
BOOTNODES_JS='
const fs = require("fs");
const net = require("net");

const skipBootnodeCheck = process.env.SKIP_BOOTNODE_CHECK === "true";

function parseMultiaddr(ma) {
  const parts = ma.split("/").filter(Boolean);
  let host = null, port = null;
  for (let i = 0; i < parts.length; i++) {
    if (["dns", "dns4", "dns6"].includes(parts[i]) && parts[i + 1]) {
      host = parts[i + 1];
    } else if (parts[i] === "ip4" && parts[i + 1]) {
      host = parts[i + 1];
    } else if (parts[i] === "tcp" && parts[i + 1]) {
      port = parseInt(parts[i + 1], 10);
    }
  }
  return { host, port };
}

function testBootnode(ma, timeoutMs = 5000) {
  return new Promise((resolve) => {
    const { host, port } = parseMultiaddr(ma);
    if (!host || !port) {
      resolve({ ma, healthy: false, reason: "unparseable" });
      return;
    }
    const socket = net.createConnection({ host, port, timeout: timeoutMs });
    socket.on("connect", () => {
      socket.destroy();
      resolve({ ma, healthy: true });
    });
    socket.on("timeout", () => {
      socket.destroy();
      resolve({ ma, healthy: false, reason: "timeout" });
    });
    socket.on("error", (err) => {
      socket.destroy();
      resolve({ ma, healthy: false, reason: err.code || err.message });
    });
  });
}

(async () => {
  const specPath = process.argv[1];
  const spec = JSON.parse(fs.readFileSync(specPath, "utf8"));

  if (skipBootnodeCheck) {
    console.log("  Bootnode health check SKIPPED, keeping existing bootnodes.");
    console.log("  Bootnodes (unchanged): " + spec.bootNodes.length);
    return;
  }

  const candidates = JSON.parse(process.env.BOOTNODES);
  console.log("  Testing " + candidates.length + " bootnodes (5s timeout each)...");
  const results = await Promise.all(candidates.map((bn) => testBootnode(bn)));
  const healthy = [];
  for (const r of results) {
    const short = r.ma.length > 80 ? r.ma.substring(0, 77) + "..." : r.ma;
    if (r.healthy) {
      console.log("    ok " + short);
      healthy.push(r.ma);
    } else {
      console.log("    x  " + short + " (" + r.reason + ")");
    }
  }
  console.log("  Healthy: " + healthy.length + "/" + candidates.length);
  if (healthy.length === 0) {
    console.log("  WARNING: No healthy bootnodes found, keeping original.");
  } else {
    spec.bootNodes = healthy;
    fs.writeFileSync(specPath, JSON.stringify(spec));
  }
  console.log("  Bootnodes: " + spec.bootNodes.length);
})();
'

# Corroborate a relay's warp-sync checkpoint against endpoints other than the one that served it.
#
# The checkpoint is what a light client trusts in place of syncing from genesis, so it is never taken
# on one endpoint's word. Each peer in PEER_RPCS is asked to confirm two things about the block
# CHECKPOINT_HEADER pins (its first 32 bytes are the parent hash, the compact integer after them the
# block number):
#
#   * The block itself, by hash. The header is hashed with blake2b-256 and compared against what the
#     peer reports for that height, which covers every field it contains. Comparing only its parent
#     hash would not: siblings share a parent, so a header with a forged state root and a genuine
#     parent would pass. Finalized blocks do not reorg, so a peer that answers and disagrees is
#     serving a different chain.
#   * The GRANDPA authorities at that block, which is what finality is verified against, since a
#     genuine header paired with a forged authority set would otherwise pass. They are read with a
#     runtime call at the hash derived above, so both sides describe the same block and compare byte
#     for byte, with no set-id bookkeeping. That call also keeps the response small; asking a peer
#     for its whole chain spec would return the raw genesis storage with it.
#
# Corroboration requires one peer that confirms both. A peer that is unreachable, behind the pinned
# height, or unable to serve the authorities is skipped rather than accepted: treating an
# unanswerable `state_call` as agreement would let any authority set through, and public endpoints
# refuse it often enough that this is a normal path, not a rare one. If no peer confirms both, the
# refresh fails rather than accept the primary alone. Only a network configured with a single
# endpoint is allowed through uncorroborated, because there is nothing to ask.
CHECKPOINT_JS='
const header = process.env.CHECKPOINT_HEADER.replace(/^0x/, "");
const authoritySet = (process.env.CHECKPOINT_AUTHORITY_SET || "").replace(/^0x/, "");
const peers = JSON.parse(process.env.PEER_RPCS);
// blake2b-256 of the SCALE header, computed by the caller: the crypto module in node
// offers blake2b512 and blake2s256 but not the 256-bit blake2b Substrate uses.
const blockHash = process.env.CHECKPOINT_BLOCK_HASH;

// SCALE compact integer at a byte offset, with the width it occupies.
function compactAt(hex, offset) {
  const byte = parseInt(hex.slice(offset * 2, offset * 2 + 2), 16);
  const mode = byte & 0b11;
  const width = mode === 0 ? 1 : mode === 1 ? 2 : mode === 2 ? 4 : 0;
  if (width === 0) throw new Error("big-integer compact values are not supported");
  const le = hex.slice(offset * 2, offset * 2 + width * 2);
  const bytes = le.match(/../g).map((b) => parseInt(b, 16));
  let value = 0;
  for (let i = bytes.length - 1; i >= 0; i--) value = value * 256 + bytes[i];
  return { value: value >>> 2, width };
}

// A GRANDPA AuthoritySet is current_authorities (32-byte public key plus 8-byte weight each)
// followed by the set id and pending changes. Its leading authority vector is encoded exactly as
// GrandpaApi_grandpa_authorities returns it, so that prefix is what a peer can be asked to confirm.
function checkpointAuthorities() {
  if (authoritySet.length === 0) return null;
  const count = compactAt(authoritySet, 0);
  const end = count.width + count.value * 40;
  if (authoritySet.length < end * 2) return null;
  return { hex: authoritySet.slice(0, end * 2), count: count.value };
}

async function rpc(url, method, params) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id: 1, jsonrpc: "2.0", method, params }),
    signal: AbortSignal.timeout(Number(process.env.TIMEOUT || 30) * 1000),
  });
  const body = await response.json();
  if (body.error) throw new Error(body.error.message || "RPC error");
  return body.result;
}

// Compare the authorities the peer reports at the block the checkpoint pins.
//
// Returns "agree", "disagree", or "unavailable". A peer that cannot serve them (pruned state, a
// node that does not expose state_call, a blip) is not evidence either way, so it must not count as
// agreement: treating silence as assent is what let an all-zero authority set pass.
async function compareAuthorities(peer, blockHash, ours) {
  let theirs;
  try {
    const raw = await rpc(peer, "state_call", ["GrandpaApi_grandpa_authorities", "0x", blockHash]);
    theirs = raw ? raw.replace(/^0x/, "") : "";
  } catch (error) {
    console.log("    ? " + peer + " served no authorities (" + (error.message || error) + ")");
    return "unavailable";
  }
  if (theirs.length === 0) {
    console.log("    ? " + peer + " served no authorities");
    return "unavailable";
  }
  if (theirs !== ours.hex) {
    console.log("    x " + peer + " reports different GRANDPA authorities at the pinned block");
    console.log("      checkpoint carries " + ours.count + " authorities");
    console.log("      peer reports " + compactAt(theirs, 0).value);
    return "disagree";
  }
  console.log("    ok " + ours.count + " GRANDPA authorities agree with " + peer);
  return "agree";
}

(async () => {
  const number = compactAt(header, 32).value;
  const ours = checkpointAuthorities();
  if (ours === null) {
    console.log("    ? checkpoint carries no readable authority set");
  }

  for (const peer of peers) {
    let peerBlockHash;
    try {
      peerBlockHash = await rpc(peer, "chain_getBlockHash", [number]);
      if (!peerBlockHash) {
        console.log("    ? " + peer + " has not reached block " + number);
        continue;
      }
    } catch (error) {
      console.log("    ? " + peer + " did not answer (" + (error.message || error) + ")");
      continue;
    }

    // The hash of the checkpoint itself, not of its parent. A parent hash is shared by every sibling at that
    // height, so comparing it leaves the state root, extrinsics root and digest unchecked: a header
    // with a forged state root and a genuine parent passes. Comparing the hash covers every field.
    if (peerBlockHash.toLowerCase() !== blockHash.toLowerCase()) {
      console.log("    x " + peer + " disagrees at block " + number);
      console.log("      checkpoint block " + blockHash);
      console.log("      peer block       " + peerBlockHash);
      process.exit(1);
    }

    if (ours !== null) {
      // Ask at the hash we derived rather than the one the peer reported. They are equal by now,
      // but reading from ours keeps the authorities tied to the block being pinned.
      const verdict = await compareAuthorities(peer, blockHash, ours);
      if (verdict === "disagree") {
        process.exit(1);
      }
      if (verdict === "unavailable") {
        // The block matched, but that is only half the checkpoint. Try the next peer.
        continue;
      }
    }

    console.log("    ok corroborated at block " + number + " by " + peer);
    return;
  }

  if (peers.length > 0) {
    console.log(
      "  ERROR: none of the " +
        peers.length +
        " other endpoint(s) confirmed both the block and the authorities at " +
        number +
        ". They were unreachable, behind that height, or could not serve the authorities; either" +
        " way the checkpoint rests on one endpoint alone, which is also how a checkpoint pinned" +
        " ahead of every peer would look.",
    );
    process.exit(1);
  }
  console.log("  WARNING: only one endpoint is configured, so the checkpoint cannot be corroborated.");
})();
'

# Fetch the genesis state root.
fetch_state_root() {
  local rpc="$1"
  local block0
  block0=$(curl -s --max-time "$TIMEOUT" -H "Content-Type: application/json" \
    -d '{"id":1,"jsonrpc":"2.0","method":"chain_getBlockHash","params":[0]}' "$rpc" 2>/dev/null \
    | jq -r '.result // empty' 2>/dev/null)
  [ -z "$block0" ] && return 1
  curl -s --max-time "$TIMEOUT" -H "Content-Type: application/json" \
    -d "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"chain_getHeader\",\"params\":[\"$block0\"]}" "$rpc" 2>/dev/null \
    | jq -r '.result.stateRoot // empty' 2>/dev/null
}

# Refresh a spec from its live chain.
#
# Always sets genesis.stateRootHash from the chain's block 0, so the spec keeps matching the chain's
# genesis after a wipe. smoldot derives the block-announces protocol name from the genesis hash, so
# a stale genesis yields a name no peer offers, the substream fails with ProtocolNotAvailable, and
# smoldot can't sync the chain. sync_state_genSyncSpec is not used for the genesis, as it returns a
# genesis that serializes extra storage keys, so its computed hash does not match the real block 0.
#
# For a relay it also fetches sync_state_genSyncSpec and writes a fresh lightSyncState checkpoint
# for smoldot to warp-sync from (a relay has no parent to follow). A parachain follows its relay
# instead, so any committed lightSyncState is dropped. If that response carries bootNodes, they are
# health-checked and pruned to the reachable ones; otherwise existing bootNodes are preserved.
#
# Pass one or more RPC URLs; the first that serves block 0 is used.
refresh_spec() {
  local spec_file="$1"
  local is_relay="$2"
  shift 2

  echo "Refreshing $spec_file..."

  local fields="" rpc="" state_root="" peers=()
  for candidate in "$@"; do
    if [ -n "$state_root" ]; then
      peers+=("$candidate")
      continue
    fi
    state_root=$(fetch_state_root "$candidate") || true
    if [ -n "$state_root" ]; then
      rpc="$candidate"
      continue
    fi
    echo "  No block 0 from $candidate"
  done
  if [ -z "$state_root" ]; then
    echo "  ERROR: Could not fetch genesis state root for $spec_file from any RPC."
    return 1
  fi
  fields+="genesis.stateRootHash"

  # Every reachable endpoint must agree on the genesis, so one endpoint alone cannot redirect a
  # spec at a different chain.
  for peer in "${peers[@]:-}"; do
    [ -z "$peer" ] && continue
    local peer_root
    peer_root=$(fetch_state_root "$peer") || true
    if [ -n "$peer_root" ] && [ "$peer_root" != "$state_root" ]; then
      echo "  ERROR: $peer serves genesis state root $peer_root, $rpc serves $state_root."
      INTEGRITY_FAILURES=$((INTEGRITY_FAILURES + 1))
      return 1
    fi
  done

  # Relays read sync_state_genSyncSpec for their checkpoint; the same response also carries the
  # bootNodes. Pull only those two fields; jq drops the multi-MB genesis the response also returns.
  local light_sync_state="null" bootnodes="[]"
  if [ "$is_relay" = "true" ]; then
    local fresh
    fresh=$(curl -s --max-time "$TIMEOUT" -H "Content-Type: application/json" \
      -d '{"id":1,"jsonrpc":"2.0","method":"sync_state_genSyncSpec","params":[true]}' "$rpc" 2>/dev/null \
      | jq -c '{lightSyncState: .result.lightSyncState, bootNodes: .result.bootNodes}' 2>/dev/null || echo "null")
    light_sync_state=$(echo "$fresh" | jq -c '.lightSyncState // null')
    bootnodes=$(echo "$fresh" | jq -c '.bootNodes // []')
    # Without lightSyncState, smoldot can't sync a relay from a stateRootHash-only genesis, so fail.
    if [ "$light_sync_state" = "null" ]; then
      echo "  ERROR: Could not fetch lightSyncState from $rpc."
      return 1
    fi
    # The checkpoint is what the light client trusts instead of syncing from genesis, so a second
    # endpoint has to agree on the block it pins before it is compiled into the wasm.
    local checkpoint_header
    checkpoint_header=$(echo "$light_sync_state" | jq -r '.finalizedBlockHeader // .finalized_block_header // empty')
    if [ -z "$checkpoint_header" ]; then
      echo "  ERROR: lightSyncState from $rpc carries no finalized block header."
      return 1
    fi
    local checkpoint_authorities checkpoint_block_hash
    checkpoint_authorities=$(echo "$light_sync_state" | jq -r '.grandpaAuthoritySet // empty')
    checkpoint_block_hash=$(HEADER="$checkpoint_header" python3 -c '
import hashlib, os, sys
header = os.environ["HEADER"].removeprefix("0x")
try:
    raw = bytes.fromhex(header)
except ValueError:
    sys.exit("the checkpoint header is not hex")
print("0x" + hashlib.blake2b(raw, digest_size=32).hexdigest())
')
    if ! CHECKPOINT_HEADER="$checkpoint_header" \
      CHECKPOINT_BLOCK_HASH="$checkpoint_block_hash" \
      CHECKPOINT_AUTHORITY_SET="$checkpoint_authorities" \
      PEER_RPCS="$(printf '%s\n' "${peers[@]:-}" | jq -R . | jq -sc 'map(select(length > 0))')" \
      TIMEOUT="$TIMEOUT" node -e "$CHECKPOINT_JS"; then
      echo "  ERROR: the checkpoint from $rpc could not be corroborated; leaving $spec_file alone."
      INTEGRITY_FAILURES=$((INTEGRITY_FAILURES + 1))
      return 1
    fi
    fields+=" + lightSyncState"
  fi

  # lightSyncState can be hundreds of KB, so it goes via stdin; the small state root goes via env.
  if ! echo "$light_sync_state" | STATE_ROOT="$state_root" \
    node -e '
      const fs = require("fs");
      let stdin = "";
      process.stdin.on("data", (chunk) => stdin += chunk);
      process.stdin.on("end", () => {
        const specPath = process.argv[1];
        const spec = JSON.parse(fs.readFileSync(specPath, "utf8"));
        spec.genesis = { stateRootHash: process.env.STATE_ROOT };
        const lss = JSON.parse(stdin);
        if (lss) spec.lightSyncState = lss;
        else delete spec.lightSyncState;
        fs.writeFileSync(specPath, JSON.stringify(spec));
      });
    ' "$SPECS_DIR/$spec_file"; then
    echo "  ERROR: failed to write $spec_file; leaving it alone."
    INTEGRITY_FAILURES=$((INTEGRITY_FAILURES + 1))
    return 1
  fi

  # Health-check bootNodes only when the chain actually advertises some. A failure
  # here leaves the already-written genesis and checkpoint in place, so it is
  # reported without discarding them.
  if [ "$(echo "$bootnodes" | jq 'length')" -gt 0 ]; then
    if BOOTNODES="$bootnodes" node -e "$BOOTNODES_JS" "$SPECS_DIR/$spec_file"; then
      fields+=" + bootNodes"
    else
      echo "  WARNING: the bootnode health check failed; $spec_file keeps its existing bootnodes."
    fi
  fi

  echo "  Updated $spec_file: $fields"
  echo ""
}

# A single network's outage should not block refreshing the others, so each call is non-fatal.

# Paseo Next v2 (the network dotli production runs on).
refresh_spec "paseo.json"                     true  "https://paseo-rpc.n.dwellir.com" \
                                                    "https://rpc.interweb-it.com/paseo" \
                                                    "https://rpc-paseo.stakeworld.io" || true
refresh_spec "paseo-next-v2-asset-hub.json"   false "https://paseo-asset-hub-next-rpc.polkadot.io" || true
refresh_spec "paseo-next-v2-bulletin.json"    false "https://paseo-bulletin-next-rpc.polkadot.io" || true
refresh_spec "paseo-next-v2-people.json"      false "https://paseo-people-next-system-rpc.polkadot.io" || true

# Paseo system parachains, on the relay spec refreshed above.
refresh_spec "paseo-asset-hub.json"           false "https://asset-hub-paseo-rpc.n.dwellir.com" \
                                                    "https://sys.turboflakes.io/asset-hub-paseo" || true
refresh_spec "paseo-bulletin.json"            false "https://bulletin-paseo.tservices.es:8443" || true
refresh_spec "paseo-people.json"              false "https://people-paseo.gatotech.network" \
                                                    "https://people-paseo.rotko.net" || true

# Previewnet.
refresh_spec "previewnet.json"                true  "https://previewnet.substrate.dev/relay/alice" \
                                                    "https://previewnet.substrate.dev/relay/bob" \
                                                    "https://previewnet.substrate.dev/relay/charlie" || true
refresh_spec "previewnet-asset-hub.json"      false "https://previewnet.substrate.dev/asset-hub" || true
refresh_spec "previewnet-bulletin.json"       false "https://previewnet.substrate.dev/bulletin" || true
refresh_spec "previewnet-people.json"         false "https://previewnet.substrate.dev/people" || true

if [ "$INTEGRITY_FAILURES" -gt 0 ]; then
  echo "FAILED: $INTEGRITY_FAILURES spec(s) could not be trusted, either because endpoints"
  echo "disagreed about the chain they serve or because no second endpoint could corroborate the"
  echo "checkpoint. Those specs were left unchanged. Investigate before publishing this run."
  exit 1
fi

echo "Done. Bundled chain specs updated in rust/crates/truapi-provider/networks/"
echo "Publish a new @parity/truapi-provider so consumers pick up the refresh."
