#!/usr/bin/env bash
# Run the bench app built by `scripts/build-ios.sh <variant> sim` on a
# simulator and keep its result lines in results/ios-sim/<variant>.jsonl.
#
#   scripts/run-ios-sim.sh <bounds|guard> [quick|full] [simulator name]
#
# The simulator runs on the Mac's CPU and kernel: it checks that everything
# builds, links and runs under the iOS runtime, not iPhone speed or iOS's
# address-space limits.
set -euo pipefail
cd "$(dirname "$0")/.."

variant=${1:?usage: run-ios-sim.sh <bounds|guard> [quick|full] [simulator name]}
suite=${2:-quick}
name=${3:-iPhone 17 Pro}
app=build/xcode-$variant/Release-iphonesimulator/QJSBench.app
[ -d "$app" ] || { echo "missing $app: run scripts/build-ios.sh $variant sim" >&2; exit 1; }

udid=$(xcrun simctl list devices available -j | python3 -c '
import json, sys
name = sys.argv[1]
devices = [d for runtime in json.load(sys.stdin)["devices"].values() for d in runtime if d["name"] == name]
booted = [d for d in devices if d["state"] == "Booted"]
print((booted or devices)[0]["udid"])
' "$name")
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl install "$udid" "$app"

out=results/ios-sim
mkdir -p "$out"
xcrun simctl launch --console-pty --terminate-running-process "$udid" io.parity.qjsbench \
  --suite "$suite" --exit 2>&1 | tee "$out/$variant.log" | grep --line-buffered '^BENCH ' | sed 's/^BENCH //' \
  | tee "$out/$variant.jsonl"
