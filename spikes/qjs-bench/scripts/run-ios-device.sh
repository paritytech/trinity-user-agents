#!/usr/bin/env bash
# Run the bench app built by `scripts/build-ios.sh <variant> device <team>`
# on a connected iPhone and keep its result lines in
# results/ios-device/<model>-<variant>-<label>.jsonl.
#
#   scripts/run-ios-device.sh <device-udid> <bounds|guard> [quick|full] [label]
#
# The phone must be unlocked, with Developer Mode on. The app keeps the
# screen awake while it runs and exits when the suite is done.
set -euo pipefail
cd "$(dirname "$0")/.."

device=${1:?usage: run-ios-device.sh <device-udid> <bounds|guard> [quick|full] [label]}
variant=${2:?usage: run-ios-device.sh <device-udid> <bounds|guard> [quick|full] [label]}
suite=${3:-quick}
label=${4:-run}
app=build/xcode-$variant/Release-iphoneos/QJSBench.app
[ -d "$app" ] || { echo "missing $app: run scripts/build-ios.sh $variant device <team>" >&2; exit 1; }

model=$(xcrun devicectl device info details --device "$device" 2>/dev/null | awk -F': ' '/Product Type:/ {print $2; exit}')
xcrun devicectl device install app --device "$device" "$app" >/dev/null

out=results/ios-device
mkdir -p "$out"
name="$out/${model:-device}-$variant-$label"
xcrun devicectl device process launch --device "$device" --console --terminate-existing \
  io.parity.qjsbench --suite "$suite" --exit 2>&1 | tee "$name.log" | grep --line-buffered '^BENCH ' \
  | sed 's/^BENCH //' | tee "$name.jsonl"
