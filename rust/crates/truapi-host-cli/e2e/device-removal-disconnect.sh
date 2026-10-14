#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
BIN="$ROOT/target/debug/truapi-host"
SCRIPT="$ROOT/rust/crates/truapi-host-cli/js/scripts/device-removal-disconnect.ts"
PRODUCT_ID="${PRODUCT_ID:-truapi-playground.dot}"
NETWORK="${TRUAPI_E2E_NETWORK:-paseo-next-v2}"
TIMEOUT_SECONDS="${TRUAPI_E2E_TIMEOUT_SECONDS:-300}"

[ -x "$BIN" ] || { echo "missing $BIN, run: cargo build -p truapi-host-cli" >&2; exit 2; }
[ -f "$ROOT/js/packages/truapi/src/generated/index.ts" ] || {
  echo "missing generated TypeScript client, run: make codegen" >&2
  exit 2
}
command -v bun >/dev/null || { echo "bun is required" >&2; exit 2; }
command -v tmux >/dev/null || { echo "tmux is required" >&2; exit 2; }

PAIRING_BASE="$(mktemp -d /tmp/truapi-device-remove-pairing.XXXXXX)"
LOG_DIR="$(mktemp -d /tmp/truapi-device-remove-logs.XXXXXX)"
SIGNER_BASE="${TRUAPI_HOST_BASE_PATH:-$(mktemp -d /tmp/truapi-device-remove-signer.XXXXXX)}"
SIGNER_BASE_OWNED=1
if [ -n "${TRUAPI_HOST_BASE_PATH:-}" ]; then
  SIGNER_BASE_OWNED=0
fi
PAIRING_LOG="$LOG_DIR/pairing.log"
SIGNING_LOG="$LOG_DIR/signing.log"
TMUX_SESSION="truapi-device-remove-$$"
PAIRING_PID=""
CORE_STORAGE=""

process_running() {
  local process_id="$1"
  local state
  state="$(ps -p "$process_id" -o stat= 2>/dev/null || true)"
  [ -n "$state" ] && [ "${state#Z}" = "$state" ]
}

stop_process() {
  local process_id="$1"
  [ -n "$process_id" ] || return 0
  pkill -TERM -P "$process_id" 2>/dev/null || true
  kill -TERM "$process_id" 2>/dev/null || true
  wait "$process_id" 2>/dev/null || true
}

capture_signing_host() {
  tmux capture-pane -p -J -S - -t "$TMUX_SESSION" >"$SIGNING_LOG"
}

stop_signing_host() {
  tmux kill-session -t "$TMUX_SESSION" 2>/dev/null || true
}

cleanup() {
  local status=$?
  if [ "$status" -ne 0 ] && tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    capture_signing_host || true
  fi
  stop_signing_host
  stop_process "$PAIRING_PID"
  if [ "$status" -eq 0 ]; then
    rm -rf -- "$PAIRING_BASE" "$LOG_DIR"
    if [ "$SIGNER_BASE_OWNED" -eq 1 ]; then
      rm -rf -- "$SIGNER_BASE"
    fi
  else
    echo "E2E logs preserved at $LOG_DIR" >&2
    echo "Pairing state preserved at $PAIRING_BASE" >&2
    if [ "$SIGNER_BASE_OWNED" -eq 1 ]; then
      echo "Signing state preserved at $SIGNER_BASE" >&2
    fi
  fi
  return "$status"
}
trap cleanup EXIT

wait_for_pairing_pattern() {
  local pattern="$1"
  local previous_count="${2:-0}"
  local deadline=$((SECONDS + TIMEOUT_SECONDS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    local current_count
    current_count="$(grep -cE "$pattern" "$PAIRING_LOG" || true)"
    if [ "$current_count" -gt "$previous_count" ]; then
      return 0
    fi
    if ! process_running "$PAIRING_PID"; then
      echo "pairing host exited before another match for $pattern" >&2
      return 1
    fi
    sleep 1
  done
  echo "timed out waiting for another match for $pattern in $PAIRING_LOG" >&2
  return 1
}

wait_for_signing_pattern() {
  local pattern="$1"
  local deadline=$((SECONDS + TIMEOUT_SECONDS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if ! tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
      echo "signing host exited before matching $pattern" >&2
      return 1
    fi
    capture_signing_host
    if grep -qE "$pattern" "$SIGNING_LOG"; then
      return 0
    fi
    sleep 1
  done
  echo "timed out waiting for $pattern in signing-host pane" >&2
  return 1
}

send_signing_command() {
  tmux send-keys -t "$TMUX_SESSION" -l "$1"
  tmux send-keys -t "$TMUX_SESSION" Enter
}

wait_for_persisted_auth_session() {
  local current_user_path="$PAIRING_BASE/v2/$NETWORK/pairing-host/current-user"
  local deadline=$((SECONDS + TIMEOUT_SECONDS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if [ -s "$current_user_path" ]; then
      local current_user
      current_user="$(tr -d '\r\n' <"$current_user_path")"
      CORE_STORAGE="$PAIRING_BASE/v2/$NETWORK/${current_user}_pairing_host/core-storage.json"
      if [ -f "$CORE_STORAGE" ] && grep -qE '"00"[[:space:]]*:' "$CORE_STORAGE"; then
        return 0
      fi
    fi
    if ! process_running "$PAIRING_PID"; then
      echo "pairing host exited before persisting its auth session" >&2
      return 1
    fi
    sleep 1
  done
  echo "timed out waiting for the persisted pairing-host auth session" >&2
  return 1
}

wait_for_auth_session_clear() {
  local deadline=$((SECONDS + TIMEOUT_SECONDS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if [ -f "$CORE_STORAGE" ] && ! grep -qE '"00"[[:space:]]*:' "$CORE_STORAGE"; then
      return 0
    fi
    if ! process_running "$PAIRING_PID"; then
      echo "pairing host exited before clearing its persisted auth session" >&2
      return 1
    fi
    sleep 1
  done
  echo "timed out waiting for AuthSession key 00 to leave $CORE_STORAGE" >&2
  return 1
}

TRUAPI_HOST_NO_UPDATE=1 NO_COLOR=1 "$BIN" pairing-host \
  --product-id "$PRODUCT_ID" \
  --network "$NETWORK" \
  --script "$SCRIPT" \
  --base-path "$PAIRING_BASE" \
  --auto-accept >"$PAIRING_LOG" 2>&1 &
PAIRING_PID=$!

wait_for_pairing_pattern 'polkadotapp://pair\?handshake=[[:xdigit:]]+'
deeplink="$(grep -m1 -oE 'polkadotapp://pair\?handshake=[[:xdigit:]]+' "$PAIRING_LOG")"

printf -v signing_command '%q ' \
  env -u HOST_CLI_SIGNER_MNEMONIC TRUAPI_HOST_NO_UPDATE=1 NO_COLOR=1 \
  "$BIN" signing-host \
  --network "$NETWORK" \
  --base-path "$SIGNER_BASE" \
  --auto-accept
tmux new-session -d -s "$TMUX_SESSION" -x 240 -y 100 -c "$ROOT" "$signing_command"
tmux set-option -t "$TMUX_SESSION" history-limit 10000 >/dev/null

wait_for_signing_pattern 'TrUAPI signing host'
send_signing_command "/pair $deeplink"
wait_for_pairing_pattern '^DEVICE_REMOVE_CONNECTED$'
wait_for_persisted_auth_session

send_signing_command '/devices'
wait_for_signing_pattern 'Paired devices for session'
capture_signing_host
device_ids=()
while IFS= read -r device_id; do
  device_ids+=("$device_id")
done < <(
  sed -nE 's/^.*(0x[[:xdigit:]]{64})  .*/\1/p' "$SIGNING_LOG" | sort -u
)
if [ "${#device_ids[@]}" -ne 1 ]; then
  echo "expected exactly one listed paired device, found ${#device_ids[@]}" >&2
  exit 1
fi

send_signing_command "/devices --remove ${device_ids[0]}"
wait_for_signing_pattern 'Remove paired device'
disconnect_marker_before_removal="$(grep -cE '^DEVICE_REMOVE_DISCONNECT_OK$' "$PAIRING_LOG" || true)"
pairing_ended_before_removal="$(grep -c 'Pairing ended' "$PAIRING_LOG" || true)"
if [ "$disconnect_marker_before_removal" -ne 0 ]; then
  echo "pairing host disconnected before removal was confirmed" >&2
  exit 1
fi
tmux send-keys -t "$TMUX_SESSION" y
wait_for_signing_pattern 'Paired device removed'

wait_for_pairing_pattern '^DEVICE_REMOVE_DISCONNECT_OK$' "$disconnect_marker_before_removal"
wait_for_pairing_pattern 'Pairing ended' "$pairing_ended_before_removal"
wait_for_auth_session_clear

send_signing_command '/devices'
wait_for_signing_pattern 'No paired devices for session'

echo "DEVICE_REMOVE_E2E_OK"
