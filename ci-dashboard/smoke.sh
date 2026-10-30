#!/usr/bin/env bash
#
# Daily smoke checks of what we publish, the way a user gets it:
#
#   ci-dashboard/smoke.sh
#
# - the CLI install command from the docs installs the stable version and runs
# - each npm package in config.json installs from the registry and imports
# - the fixed Android nightly link serves an APK of the nightly app, at the
#   build number of the newest Android nightly release
#
# Prints a Markdown table, appends it to the job summary when there is one,
# and exits non-zero if any check failed. Needs bash, curl, jq, node, npm and
# aapt from the Android SDK. GITHUB_TOKEN, when set, is used for the releases
# API instead of the anonymous limit.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
config="${here}/config.json"
repo="$(jq -r .repository "${config}")"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
# npm's cache goes with the rest, so daily runs do not fill the disk.
export npm_config_cache="${work}/npm-cache"

# A stalled download fails its check rather than hanging the run: curl gives
# up on a transfer slower than 1 KB/s for a minute, and every step that runs
# what was downloaded gets ten minutes where `timeout` exists, as it does in
# the image.
curl() { command curl --connect-timeout 30 --speed-limit 1024 --speed-time 60 "$@"; }
limited() {
  if command -v timeout >/dev/null; then
    timeout --kill-after=30 "${SMOKE_STEP_LIMIT:-600}" "$@"
  else
    "$@"
  fi
}

rows=()
failed=0

record() {
  local ok="$1" name="$2" detail="$3"
  if [ "${ok}" = 1 ]; then
    rows+=("| ✅ | ${name} | ${detail} |")
  else
    rows+=("| ❌ | ${name} | ${detail} |")
    failed=1
  fi
}

github_api() {
  local auth=()
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    auth=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
  fi
  curl -fsSL ${auth[@]+"${auth[@]}"} -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${repo}/$1"
}

# --- CLI ----------------------------------------------------------------------

check_cli() {
  local stable installed
  stable="$(curl -fsSL "https://github.com/${repo}/releases/download/truapi-host-cli-stable/version?t=$(date +%s)" | tr -d '[:space:]')"
  if ! curl -fsSL "https://raw.githubusercontent.com/${repo}/main/scripts/truapi-host-installer.sh" |
    TRUAPI_HOST_INSTALL_DIR="${work}/cli" TRUAPI_HOST_BIN_DIR="${work}/bin" limited bash >"${work}/cli.log" 2>&1; then
    record 0 "CLI install" "installer failed: $(tail -1 "${work}/cli.log")"
    return
  fi
  if ! installed="$(limited "${work}/bin/truapi-host" --version 2>&1)"; then
    record 0 "CLI install" "installed binary does not run: ${installed}"
    return
  fi
  if [ "${installed}" = "truapi-host ${stable}" ]; then
    record 1 "CLI install" "installs and runs ${installed}"
  else
    record 0 "CLI install" "runs \`${installed}\`, stable is ${stable}"
  fi
}

# --- npm ----------------------------------------------------------------------

check_npm() {
  local package version
  mkdir -p "${work}/npm"
  (cd "${work}/npm" && npm init -y >/dev/null)
  while read -r package; do
    # --ignore-scripts: a package's install scripts are not what a user's
    # import runs, and they would run here unchecked.
    if ! (cd "${work}/npm" && limited npm install --ignore-scripts --no-audit --no-fund --silent "${package}" >"${work}/npm.log" 2>&1); then
      record 0 "\`${package}\`" "npm install failed: $(tail -1 "${work}/npm.log")"
      continue
    fi
    version="$(jq -r .version "${work}/npm/node_modules/${package}/package.json")"
    if (cd "${work}/npm" && limited node --input-type=module -e "await import('${package}')" >"${work}/import.log" 2>&1); then
      record 1 "\`${package}\`" "${version} installs and imports"
    else
      record 0 "\`${package}\`" "${version} does not import: $(tail -1 "${work}/import.log")"
    fi
  done < <(jq -r '.smoke.npmPackages[]' "${config}")
}

# --- Android nightly APK ------------------------------------------------------

aapt_bin() {
  if command -v aapt >/dev/null; then
    command -v aapt
    return
  fi
  local sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-${HOME}/Library/Android/sdk}}"
  find "${sdk}/build-tools" -name aapt -type f 2>/dev/null | sort -V | tail -1
}

check_apk() {
  local url package prefix expected aapt badging name code
  url="$(jq -r .smoke.nightlyApk.url "${config}")"
  package="$(jq -r .smoke.nightlyApk.package "${config}")"
  prefix="$(jq -r .smoke.nightlyApk.tagPrefix "${config}")"
  # The backticks are literal: the notes say "Nightly build `1049`".
  # shellcheck disable=SC2016
  expected="$(github_api "releases?per_page=50" | jq -r \
    "[.[] | select(.tag_name | startswith(\"${prefix}\")) | select(.draft | not)] | sort_by(.published_at) | last | .body" |
    sed -n 's/.*Nightly build `\([0-9]*\)`.*/\1/p' | head -1)"
  aapt="$(aapt_bin)"
  if [ -z "${aapt}" ]; then
    record 0 "Android nightly APK" "aapt not found"
    return
  fi
  if ! curl -fsSL -o "${work}/nightly.apk" "${url}"; then
    record 0 "Android nightly APK" "download failed"
    return
  fi
  # Read it all and keep the first line: piping into `head -1` fails under
  # pipefail whenever aapt is still writing when head exits.
  if ! badging="$("${aapt}" dump badging "${work}/nightly.apk" 2>/dev/null)"; then
    record 0 "Android nightly APK" "not a readable APK"
    return
  fi
  badging="${badging%%$'\n'*}"
  # Anchored: the same line ends in platformBuildVersionName='…'.
  name="$(sed -n "s/^package: name='\([^']*\)'.*/\1/p" <<<"${badging}")"
  code="$(sed -n "s/.* versionCode='\([^']*\)'.*/\1/p" <<<"${badging}")"
  if [ "${name}" != "${package}" ]; then
    record 0 "Android nightly APK" "package is ${name}, expected ${package}"
  elif [ -n "${expected}" ] && [ "${code}" != "${expected}" ]; then
    record 0 "Android nightly APK" "build ${code}, newest nightly is ${expected}"
  else
    record 1 "Android nightly APK" "${name}, build ${code}"
  fi
}

check_cli
check_npm
check_apk

summary="$(printf '%s\n' "### Smoke checks" "" "| | Check | Detail |" "| --- | --- | --- |" "${rows[@]}")"
echo "${summary}"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  echo "${summary}" >>"${GITHUB_STEP_SUMMARY}"
fi
exit "${failed}"
