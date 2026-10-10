#!/usr/bin/env bash
# Builds the iOS simulator app with app/ copied into its sources, and removes the copy however
# the build ends. Arguments go to fastlane.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ios="$(cd "${here}/../../../hosts/ios" && pwd)"
dest="${ios}/polkadot-app/HostPlaygroundE2E"

rm -rf "${dest}"
trap 'rm -rf "${dest}"' EXIT
mkdir -p "${dest}"
cp "${here}"/app/* "${dest}/"

cd "${ios}"
bundle exec fastlane build_app_simulator "$@"
