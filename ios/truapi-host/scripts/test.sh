#!/bin/bash
set -euo pipefail

TRUAPI_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$TRUAPI_ROOT"

generator="$TRUAPI_ROOT/target/tools/xcodegen/2.46.0"
if [[ ! -x "$generator/xcodegen/bin/xcodegen" ]]; then
  mkdir -p "$generator"
  curl --fail --location --silent --show-error \
    https://github.com/yonaskolb/XcodeGen/releases/download/2.46.0/xcodegen.zip \
    --output "$generator/xcodegen.zip"
  echo "4d9e34b62172d645eed6457cac13fc222569974098ef4ee9c3368bedf0196806  $generator/xcodegen.zip" | shasum -a 256 -c -
  unzip -q -o "$generator/xcodegen.zip" -d "$generator"
fi
"$generator/xcodegen/bin/xcodegen" generate --spec ios/truapi-host/TestHost/project.yml

simulator="${TRUAPI_TEST_SIMULATOR:-$(node --input-type=module -e 'import { selectSimulator } from "./scripts/lib/ios-simulator.mjs"; console.log(selectSimulator().udid)')}"
runtime="$(xcrun simctl list -j | jq -er --arg simulator "$simulator" '
  . as $available
  | .devices | to_entries[]
  | select(any(.value[]; .udid == $simulator))
  | .key as $identifier
  | $available.runtimes[]
  | select(.identifier == $identifier)
  | .bundlePath
')"
swift_libraries="$runtime/Contents/Resources/RuntimeRoot/System/Cryptexes/OS/usr/lib/swift"
# iOS 18.5 omits this overlay from its loader path: https://bugs.webkit.org/show_bug.cgi?id=293831
if [[ -f "$swift_libraries/libswiftWebKit.dylib" ]]; then
  export TEST_RUNNER_DYLD_FALLBACK_LIBRARY_PATH="$swift_libraries"
fi

if [[ $# -eq 0 ]]; then set -- test; fi
xcodebuild "$@" \
  -project ios/truapi-host/TestHost/NetworkPermissionTests.xcodeproj \
  -scheme NetworkPermissionTests \
  -destination "platform=iOS Simulator,arch=arm64,id=$simulator"
