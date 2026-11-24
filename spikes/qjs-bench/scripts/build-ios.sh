#!/usr/bin/env bash
# Build the iOS bench app for one wasm2c variant.
#
#   scripts/build-ios.sh <bounds|guard> <sim|device> [team-id]
#
# Builds BenchKit (native QuickJS, wasmi, and wasm2c with the chosen bounds
# check) for the device and simulator, wraps it in build/BenchKit.xcframework,
# copies the workloads into the app and builds QJSBench.app into
# build/xcode-<variant>/. A device build signs automatically with the team
# given, registering io.parity.qjsbench if needed.
#
# Needs: scripts/build-guest.sh, `bun run build` in workloads/, and wasm2c
# (`brew install wabt`). The two variants cannot share a binary: the wasm2c
# runtime is compiled for one of them.
set -euo pipefail
cd "$(dirname "$0")/.."

variant=${1:?usage: build-ios.sh <bounds|guard> <sim|device> [team-id]}
platform=${2:?usage: build-ios.sh <bounds|guard> <sim|device> [team-id]}
team=${3:-}
case "$variant" in bounds | guard) ;; *) echo "variant must be bounds or guard" >&2; exit 2 ;; esac

guest=target/wasm32-wasip1/release/qjs_guest.wasm
[ -f "$guest" ] || { echo "missing $guest: run scripts/build-guest.sh" >&2; exit 1; }
[ -f workloads/dist/w-worker.js ] || { echo "missing workloads/dist: run bun run build in workloads/" >&2; exit 1; }

export IPHONEOS_DEPLOYMENT_TARGET=17.0
export QJS_W2C_MEMCHECK=$variant
# A static library keeps its exported symbols only unstripped; Xcode strips the app.
export CARGO_PROFILE_RELEASE_STRIP=false
target_dir=target/ios-$variant
for triple in aarch64-apple-ios aarch64-apple-ios-sim; do
  CARGO_TARGET_DIR=$target_dir cargo build --release -p qjs-benchkit --target "$triple"
done

rm -rf build/BenchKit.xcframework
xcodebuild -create-xcframework \
  -library "$target_dir/aarch64-apple-ios/release/libqjs_benchkit.a" -headers benchkit/include \
  -library "$target_dir/aarch64-apple-ios-sim/release/libqjs_benchkit.a" -headers benchkit/include \
  -output build/BenchKit.xcframework

resources=ios-bench/QJSBench/Resources
rm -rf "$resources"
mkdir -p "$resources"
cp workloads/dist/w-worker.js workloads/dist/w-codec.js workloads/dist/w-rxjs.js \
  workloads/dist/w-pingpong-64.js workloads/dist/w-pingpong-4k.js \
  workloads/corpus/corpus.bin samples/t1-memory.js "$resources/"

symroot=$PWD/build/xcode-$variant
case "$platform" in
  sim)
    xcodebuild -project ios-bench/QJSBench.xcodeproj -target QJSBench -configuration Release \
      -sdk iphonesimulator -arch arm64 SYMROOT="$symroot" CODE_SIGN_IDENTITY=- build
    echo "built $symroot/Release-iphonesimulator/QJSBench.app"
    ;;
  device)
    [ -n "$team" ] || { echo "a device build needs the team id" >&2; exit 2; }
    xcodebuild -project ios-bench/QJSBench.xcodeproj -target QJSBench -configuration Release \
      -sdk iphoneos -arch arm64 SYMROOT="$symroot" DEVELOPMENT_TEAM="$team" \
      -allowProvisioningUpdates build
    echo "built $symroot/Release-iphoneos/QJSBench.app"
    ;;
  *) echo "platform must be sim or device" >&2; exit 2 ;;
esac
