#!/usr/bin/env bash
# Build the Android bench app for one wasm2c variant into
# build/android-app/qjsbench-<variant>.apk.
#
#   ANDROID_HOME=<sdk> ANDROID_NDK_HOME=<ndk> [JAVA_HOME=<jdk 21>] \
#     scripts/build-android-app.sh <bounds|guard>
#
# BenchKit (native QuickJS, wasm2c, wasmi, Boa) is cross-compiled into the
# app's jniLibs; the workloads go into its assets. A release build, signed
# with the debug key.
set -euo pipefail
cd "$(dirname "$0")/.."

variant=${1:?usage: build-android-app.sh <bounds|guard>}
: "${ANDROID_HOME:?set ANDROID_HOME to the SDK}"
: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME to the NDK}"
[ -f target/wasm32-wasip1/release/qjs_guest.wasm ] || { echo "run scripts/build-guest.sh first" >&2; exit 1; }

app=android-bench/app/src/main
QJS_W2C_MEMCHECK=$variant CARGO_TARGET_DIR=target/android-bench-$variant \
  cargo ndk -t arm64-v8a --platform 29 -o "$app/jniLibs" build --release -p qjs-benchkit

rm -rf "$app/assets/bench"
mkdir -p "$app/assets/bench"
cp workloads/dist/w-worker.js workloads/dist/w-codec.js workloads/dist/w-rxjs.js \
  workloads/dist/w-pingpong-64.js workloads/dist/w-pingpong-4k.js \
  workloads/corpus/corpus.bin samples/t1-memory.js "$app/assets/bench/"

echo "sdk.dir=$ANDROID_HOME" >android-bench/local.properties
(cd android-bench && ./gradlew --quiet assembleRelease)
mkdir -p build/android-app
cp android-bench/app/build/outputs/apk/release/app-release.apk "build/android-app/qjsbench-$variant.apk"
ls -l "build/android-app/qjsbench-$variant.apk"
