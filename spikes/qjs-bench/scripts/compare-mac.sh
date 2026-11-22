#!/usr/bin/env bash
# Run every engine's comparison on this Mac, three times each, into
# results/mac/<engine>-<mode>-<run>.txt. Build first: scripts/build-guest.sh,
# `cargo build --release -p qjs-runner -p qjs-runner-pulley`,
# `cargo build --release -p qjs-guest --bin native-probe`, `bun run build` in
# workloads/, and `swiftc -O -swift-version 5 -framework WebKit
# webview-mac/WebViewBench.swift -o webview-mac/webview-bench`.
set -euo pipefail
cd "$(dirname "$0")/.."

guest=target/wasm32-wasip1/release/qjs_guest.wasm
worker=workloads/dist/w-worker.js
corpus=workloads/corpus/corpus.bin
shim=workloads/dist/webview-shim.js
out=results/mac
mkdir -p "$out"

for run in 1 2 3; do
  ./target/release/native-probe "$worker" --replay "$corpus" | grep RESULT >"$out/native-replay-$run.txt"
  ./target/release/native-probe "$worker" --instances 10 | grep RESULT >"$out/native-instances-$run.txt"
  ./target/release/qjs-runner "$guest" "$worker" --native-stack-mib 1 --replay "$corpus" | grep RESULT >"$out/wasmi-replay-$run.txt"
  ./target/release/qjs-runner "$guest" "$worker" --native-stack-mib 1 --instances 10 | grep RESULT >"$out/wasmi-instances-$run.txt"
  ./target/release/qjs-runner-pulley "$guest" "$worker" --native --replay "$corpus" | grep RESULT >"$out/cranelift-replay-$run.txt"
  ./target/release/qjs-runner-pulley "$guest" "$worker" --native --instances 10 | grep RESULT >"$out/cranelift-instances-$run.txt"
  ./webview-mac/webview-bench replay "$shim" "$worker" "$corpus" | grep RESULT >"$out/wkwebview-replay-$run.txt"
  ./webview-mac/webview-bench instances "$shim" "$worker" 10 | grep RESULT >"$out/wkwebview-instances-$run.txt"
done
grep -h RESULT "$out"/*.txt
