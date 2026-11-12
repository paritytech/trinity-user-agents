#!/usr/bin/env bash
# Usage: scripts/crates-io-index.sh <crate> <version>
#
# Exits 0 once the crates.io sparse index lists <version> of <crate>, 1 when it
# does not, and 2 when the last lookup failed. Looks up once by default; with
# CRATES_INDEX_WAIT_SECONDS set, polls every CRATES_INDEX_INTERVAL_SECONDS
# (default 15) until that deadline.
set -euo pipefail

crate="$1"
version="$2"
wait_seconds="${CRATES_INDEX_WAIT_SECONDS:-0}"
interval="${CRATES_INDEX_INTERVAL_SECONDS:-15}"

case "${#crate}" in
  1) path="1/${crate}" ;;
  2) path="2/${crate}" ;;
  3) path="3/${crate:0:1}/${crate}" ;;
  *) path="${crate:0:2}/${crate:2:2}/${crate}" ;;
esac

index="$(mktemp)"
trap 'rm -f "${index}"' EXIT
deadline=$((SECONDS + wait_seconds))
while :; do
  # One line of JSON per published version; 404 for a crate never published.
  # curl prints 000 when no server answers.
  status="$(curl -sS --max-time 10 -o "${index}" -w '%{http_code}' "https://index.crates.io/${path}" || true)"
  case "${status}" in
    200)
      if jq -r '.vers' "${index}" | grep -Fxq "${version}"; then
        exit 0
      fi
      result=1
      ;;
    404) result=1 ;;
    *)
      echo "crates.io index answered ${status:-nothing} for ${crate}" >&2
      result=2
      ;;
  esac
  if ((SECONDS + interval > deadline)); then
    exit "${result}"
  fi
  sleep "${interval}"
done
