#!/usr/bin/env bash
# Run the wasm2c runner (guard pages and explicit bounds checks) beside
# native QuickJS and wasmi, three times each, into results/mac-wasm2c/.
# Build first: scripts/build-guest.sh, `bun run build` in workloads/,
# `cargo build --release -p qjs-runner -p qjs-runner-wasm2c`,
# `cargo build --release -p qjs-guest --bin native-probe`, and
# `QJS_W2C_MEMCHECK=bounds CARGO_TARGET_DIR=target/w2c-bounds cargo build --release -p qjs-runner-wasm2c`.
set -euo pipefail
cd "$(dirname "$0")/.."

guest=target/wasm32-wasip1/release/qjs_guest.wasm
worker=workloads/dist/w-worker.js
corpus=workloads/corpus/corpus.bin
guard=target/release/qjs-runner-wasm2c
bounds=target/w2c-bounds/release/qjs-runner-wasm2c
out=results/mac-wasm2c
mkdir -p "$out"

for run in 1 2 3; do
  "$guard" "$worker" --replay "$corpus" | grep RESULT >"$out/w2c-guard-replay-$run.txt"
  "$guard" "$worker" --instances 10 | grep RESULT >"$out/w2c-guard-instances-$run.txt"
  "$bounds" "$worker" --native-stack-mib 64 --replay "$corpus" | grep RESULT >"$out/w2c-bounds-replay-$run.txt"
  "$bounds" "$worker" --native-stack-mib 64 --instances 10 | grep RESULT >"$out/w2c-bounds-instances-$run.txt"
  ./target/release/native-probe "$worker" --replay "$corpus" | grep RESULT >"$out/native-replay-$run.txt"
  ./target/release/qjs-runner "$guest" "$worker" --native-stack-mib 1 --replay "$corpus" | grep RESULT >"$out/wasmi-replay-$run.txt"
done

for workload in codec rxjs pingpong-64 pingpong-4k; do
  for engine in guard bounds native wasmi; do
    # The long throughput loops outrun wasmi's default fuel, so metering is off.
    case "$engine" in
      guard) line=$("$guard" "workloads/dist/w-$workload.js" 2>&1 | grep BENCH_RESULT) ;;
      bounds) line=$("$bounds" "workloads/dist/w-$workload.js" --native-stack-mib 64 2>&1 | grep BENCH_RESULT) ;;
      native) line=$(./target/release/native-probe "workloads/dist/w-$workload.js" 2>&1 | grep BENCH_RESULT) ;;
      wasmi) line=$(./target/release/qjs-runner "$guest" "workloads/dist/w-$workload.js" --native-stack-mib 1 --fuel-per-turn 0 2>&1 | grep BENCH_RESULT) ;;
    esac
    echo "$workload $engine: $line" | tee -a "$out/throughput.txt"
  done
done
grep -h RESULT "$out"/*.txt
