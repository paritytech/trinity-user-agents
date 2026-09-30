#!/usr/bin/env bash
# Private HTTPS for the web signing host, through Tailscale Serve (tailnet only).
#
#   scripts/tailscale-serve.sh up      route the host and the product ports
#   scripts/tailscale-serve.sh down    remove the routes `up` made, and only those
#   scripts/tailscale-serve.sh status  show what is routed and what this script made
#
# One name means one certificate, so products are told apart by port: the host
# is on 443 and each product owns one port of a range. Every route forwards to
# one local server, which must already be running (see README, "Private HTTPS").
#
# `up` records each route it makes in $STATE (host name, port, target), with the
# Serve config it found before the first `up`. `down` removes a route only if it
# is recorded there and still exactly what `up` made; anything else is left
# alone and reported. `up` is safe to repeat and keeps the original snapshot. It
# refuses, and changes nothing, when one of its ports is already used by
# something else or has Funnel on. A route that already points at BACKEND but was
# not made here is left alone, and `down` leaves it too. If creating a route
# fails, `up` undoes the routes it made in that run. It never enables Funnel. The
# snapshot is for you to read; nothing replays it. A record that is malformed or
# from an older version stops both commands before any change; check `tailscale
# serve status` and delete the file.
#
# Environment: BACKEND (default http://127.0.0.1:5181), PORTS (default
# 9450-9459, not 443), STATE (default
# ${XDG_STATE_HOME:-~/.local/state}/truapi-web-signing-host/tailscale-serve.json;
# it holds your tailnet name, so keep it out of git). `down` uses the recorded
# routes and ignores BACKEND and PORTS. A lock directory, $STATE.lock, stops two
# runs at once; if a run crashed, delete it.
set -euo pipefail

BACKEND="${BACKEND:-http://127.0.0.1:5181}"
PORTS="${PORTS:-9450-9459}"
STATE="${STATE:-${XDG_STATE_HOME:-$HOME/.local/state}/truapi-web-signing-host/tailscale-serve.json}"
LOCK="$STATE.lock"
HELPER="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/tailscale_serve_state.py"

die() { echo "error: $*" >&2; exit 1; }
plan() { python3 "$HELPER" "$@"; }
serve_status() { tailscale serve status --json; }

command -v tailscale >/dev/null || die "tailscale is not installed"
command -v python3 >/dev/null || die "python3 is needed to read the Serve config"

take_lock() {
  mkdir -p "$(dirname "$STATE")"
  mkdir "$LOCK" 2>/dev/null || die "another run holds $LOCK; if none is running, delete that directory"
  trap 'rm -rf "$LOCK"' EXIT
  trap 'exit 130' INT TERM
}

node_name() {
  tailscale status --json | python3 -c '
import json, sys
d = json.load(sys.stdin)
if d.get("BackendState") != "Running": sys.exit("tailscale is not running")
name = d["Self"]["DNSName"].rstrip(".")
if name not in (d.get("CertDomains") or []): sys.exit("HTTPS certificates are not enabled for this tailnet")
print(name)'
}

check_settings() {
  local backend_re='^http://(127\.0\.0\.1|localhost):[0-9]+$' ports_re='^([0-9]+)-([0-9]+)$'
  [[ "$BACKEND" =~ $backend_re ]] || die "BACKEND must look like http://127.0.0.1:5181"
  [[ "$PORTS" =~ $ports_re ]] || die "PORTS must look like 9450-9459"
  FIRST="${BASH_REMATCH[1]}"
  LAST="${BASH_REMATCH[2]}"
  [[ "$FIRST" -le "$LAST" && "$LAST" -le 65535 ]] || die "PORTS must be an ascending range of valid ports"
  [[ "$FIRST" -gt 443 || "$LAST" -lt 443 ]] || die "PORTS may not include 443, which is the host"
}

# Switch off each given recorded port if it is still the route `up` made. A port
# stays recorded until it is confirmed gone or found changed. Returns non-zero
# if any port could not be settled.
release_ports() {
  local port out verdict detail bad=0
  for port in "$@"; do
    out="$(serve_status | plan settle "$STATE" "$port")" || { bad=1; continue; }
    read -r verdict detail <<<"$out"
    case "$verdict" in
      remove)
        if tailscale serve --https="$port" off >/dev/null \
          && [[ "$(serve_status | plan settle "$STATE" "$port")" == gone* ]]; then
          echo "removed port $port"
        else
          echo "error: could not remove port $port; see: tailscale serve status" >&2
          bad=1
        fi
        ;;
      gone) echo "port $port: already gone" ;;
      *) echo "port $port: left alone, no longer the route this script made" ;;
    esac
  done
  plan finish "$STATE" || bad=1
  return "$bad"
}

up() {
  check_settings
  take_lock
  local name status lines port kind detail create="" created="" blocked=0
  name="$(node_name)"
  status="$(serve_status)"
  lines="$(printf '%s' "$status" | plan classify "$STATE" "$name" "$BACKEND" 443 $(seq "$FIRST" "$LAST"))"
  while read -r port kind detail; do
    case "$kind" in
      free) create="$create $port" ;;
      owned) echo "port $port: already routed by this script" ;;
      existing) echo "port $port: already points at $BACKEND, not made by this script; leaving it" ;;
      *) echo "error: port $port: $detail" >&2; blocked=1 ;;
    esac
  done <<<"$lines"
  [[ "$blocked" -eq 0 ]] || die "nothing was changed"

  if [[ -n "$create" ]]; then
    printf '%s' "$status" | plan init "$STATE"
    for port in $create; do
      plan own "$STATE" "$port" "$name" "$BACKEND"
      created="$created $port"
      if ! tailscale serve --bg --https="$port" "$BACKEND" >/dev/null; then
        echo "error: tailscale serve failed on port $port; undoing the routes made in this run" >&2
        release_ports $created >&2 || echo "error: some routes are still in place and recorded; run down" >&2
        exit 1
      fi
    done
  fi
  echo "host:     https://$name/"
  echo "products: https://$name:{$FIRST-$LAST}"
  echo "sandbox origin template: https://$name:{$FIRST-$LAST}"
}

down() {
  take_lock
  local owned
  owned="$(plan ports "$STATE")"
  [[ -n "$owned" ]] || { echo "no routes are recorded as made by this script; changed nothing"; return 0; }
  release_ports $owned
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  status)
    tailscale serve status
    plan summary "$STATE"
    ;;
  *)
    sed -n '2,5p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
