#!/usr/bin/env bash
# Cross-compile every runner for 64-bit ARM Android (API 29, the app's
# minSdk) into build/android/: native QuickJS, wasmi, Cranelift, Boa and
# wasm2c with guard pages and with bounds checks.
#
#   ANDROID_NDK_HOME=<ndk> scripts/build-android.sh
#
# Needs cargo-ndk, the aarch64-linux-android Rust target, wasm2c
# (`brew install wabt`), and the guest from scripts/build-guest.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME to the NDK}"
[ -f target/wasm32-wasip1/release/qjs_guest.wasm ] || { echo "run scripts/build-guest.sh first" >&2; exit 1; }

ndk=(cargo ndk -t arm64-v8a --platform 29)
out=build/android
mkdir -p "$out"

CARGO_TARGET_DIR=target/android "${ndk[@]}" build --release -p qjs-runner -p qjs-runner-boa -p qjs-runner-pulley
CARGO_TARGET_DIR=target/android "${ndk[@]}" build --release -p qjs-guest --bin native-probe
QJS_W2C_MEMCHECK=guard CARGO_TARGET_DIR=target/android "${ndk[@]}" build --release -p qjs-runner-wasm2c
QJS_W2C_MEMCHECK=bounds CARGO_TARGET_DIR=target/android-bounds "${ndk[@]}" build --release -p qjs-runner-wasm2c

release=target/android/aarch64-linux-android/release
cp "$release/qjs-runner" "$release/qjs-runner-boa" "$release/qjs-runner-pulley" "$release/native-probe" "$out/"
cp "$release/qjs-runner-wasm2c" "$out/qjs-runner-wasm2c-guard"
cp target/android-bounds/aarch64-linux-android/release/qjs-runner-wasm2c "$out/qjs-runner-wasm2c-bounds"
ls -l "$out"
