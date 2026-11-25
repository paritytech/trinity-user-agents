#!/usr/bin/env bash

set -euo pipefail

: "${CERTIFICATE_OSX_APPLICATION:?CERTIFICATE_OSX_APPLICATION is required}"
: "${CERTIFICATE_PASSWORD:?CERTIFICATE_PASSWORD is required}"
: "${RUNNER_TEMP:?RUNNER_TEMP is required}"
: "${GITHUB_RUN_ID:?GITHUB_RUN_ID is required}"
: "${GITHUB_RUN_ATTEMPT:?GITHUB_RUN_ATTEMPT is required}"

KEY_CHAIN="${RUNNER_TEMP}/polkadot-desktop-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}.keychain-db"
KEY_CHAIN_STATE="${KEY_CHAIN}.state"
CERTIFICATE_P12="${RUNNER_TEMP}/polkadot-desktop-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}.p12"
KEY_CHAIN_PASSWORD="$(openssl rand -base64 24)"

mkdir -p "$KEY_CHAIN_STATE"
chmod 700 "$KEY_CHAIN_STATE"

# Preserve the runner's keychain configuration. This matters on the self-hosted
# parity-macos runner, where later jobs must not inherit this build keychain.
security default-keychain -d user | sed 's/^[[:space:]]*"//; s/"[[:space:]]*$//' > "$KEY_CHAIN_STATE/default"
security list-keychains -d user | sed 's/^[[:space:]]*"//; s/"[[:space:]]*$//' > "$KEY_CHAIN_STATE/list"

trap 'rm -f "$CERTIFICATE_P12"' EXIT
printf '%s' "$CERTIFICATE_OSX_APPLICATION" | base64 --decode > "$CERTIFICATE_P12"
chmod 600 "$CERTIFICATE_P12"

security delete-keychain "$KEY_CHAIN" 2>/dev/null || true
security create-keychain -p "$KEY_CHAIN_PASSWORD" "$KEY_CHAIN"
security set-keychain-settings -lut 21600 "$KEY_CHAIN"
security unlock-keychain -p "$KEY_CHAIN_PASSWORD" "$KEY_CHAIN"
security default-keychain -d user -s "$KEY_CHAIN"
security list-keychains -d user -s "$KEY_CHAIN"

security import "$CERTIFICATE_P12" \
  -k "$KEY_CHAIN" \
  -P "$CERTIFICATE_PASSWORD" \
  -T /usr/bin/codesign
security set-key-partition-list \
  -S apple-tool:,apple: \
  -s \
  -k "$KEY_CHAIN_PASSWORD" \
  "$KEY_CHAIN"

IDENTITIES="$(security find-identity -v -p codesigning "$KEY_CHAIN")"
printf '%s\n' "$IDENTITIES"
if [[ "$IDENTITIES" != *'"Developer ID Application:'* ]]; then
  echo "No Developer ID Application identity was imported." >&2
  exit 1
fi
