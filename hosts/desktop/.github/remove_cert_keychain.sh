#!/usr/bin/env bash

set -u

: "${RUNNER_TEMP:?RUNNER_TEMP is required}"
: "${GITHUB_RUN_ID:?GITHUB_RUN_ID is required}"
: "${GITHUB_RUN_ATTEMPT:?GITHUB_RUN_ATTEMPT is required}"

KEY_CHAIN="${RUNNER_TEMP}/polkadot-desktop-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}.keychain-db"
KEY_CHAIN_STATE="${KEY_CHAIN}.state"
STATUS=0

if [[ -f "$KEY_CHAIN_STATE/list" ]]; then
  ORIGINAL_KEYCHAINS=()
  while IFS= read -r keychain; do
    if [[ -n "$keychain" ]]; then
      ORIGINAL_KEYCHAINS+=("$keychain")
    fi
  done < "$KEY_CHAIN_STATE/list"

  if [[ ${#ORIGINAL_KEYCHAINS[@]} -gt 0 ]]; then
    security list-keychains -d user -s "${ORIGINAL_KEYCHAINS[@]}" || STATUS=1
  fi
fi

if [[ -s "$KEY_CHAIN_STATE/default" ]]; then
  ORIGINAL_DEFAULT="$(<"$KEY_CHAIN_STATE/default")"
  security default-keychain -d user -s "$ORIGINAL_DEFAULT" || STATUS=1
fi

security delete-keychain "$KEY_CHAIN" 2>/dev/null || rm -f "$KEY_CHAIN"
rm -rf "$KEY_CHAIN_STATE"

exit "$STATUS"
