#!/usr/bin/env bash
# Builds the iOS simulator app with the host-playground driver in it.
#
# Copies app/ into the app's sources, where the project picks it up, runs the
# simulator lane, and removes the copy however the build ends. hosts/ios is
# left as it was, and no other build of the app contains the driver. Arguments
# are passed to fastlane.
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
