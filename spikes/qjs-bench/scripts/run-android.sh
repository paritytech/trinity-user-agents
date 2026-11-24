#!/usr/bin/env bash
# Run every engine on a connected Android phone, three times each, and keep
# the lines in results/android/<model>/. Build first: scripts/build-android.sh.
#
#   [ADB=<path to adb>] scripts/run-android.sh [adb serial]
#
# The runners are plain binaries in /data/local/tmp, run from `adb shell`:
# engine speed and memory, without an app process or Binder in the way.
# Memory is the proportional set size (PSS) Android accounts processes by.
set -euo pipefail
cd "$(dirname "$0")/.."

adb=("${ADB:-adb}")
[ -n "${1:-}" ] && adb+=(-s "$1")
remote=/data/local/tmp/qjs-bench
bin=build/android
[ -x "$bin/native-probe" ] || { echo "run scripts/build-android.sh first" >&2; exit 1; }

model=$("${adb[@]}" shell getprop ro.product.model | tr -d '\r')
out=results/android/$model
mkdir -p "$out"
{
  echo "model: $model"
  echo "android: $("${adb[@]}" shell getprop ro.build.version.release | tr -d '\r')"
  echo "soc: $("${adb[@]}" shell getprop ro.soc.model | tr -d '\r')"
  echo "cores: $("${adb[@]}" shell nproc | tr -d '\r')"
} | tee "$out/device.txt"

"${adb[@]}" shell mkdir -p "$remote"
"${adb[@]}" push -q "$bin"/* target/wasm32-wasip1/release/qjs_guest.wasm workloads/corpus/corpus.bin \
  workloads/dist/w-worker.js workloads/dist/w-codec.js workloads/dist/w-rxjs.js \
  workloads/dist/w-pingpong-64.js workloads/dist/w-pingpong-4k.js samples/t1-memory.js "$remote/"
"${adb[@]}" shell chmod 755 "$remote"/native-probe "$remote"/qjs-runner*

on_device() {
  "${adb[@]}" shell "cd $remote && $1" 2>&1 | tr -d '\r'
}

thermal() {
  "${adb[@]}" shell dumpsys thermalservice 2>/dev/null | tr -d '\r' | awk -F': ' '/Thermal Status/ {print $2; exit}'
}

# ENGINES="boa cranelift" reruns a subset; each engine's lines are its own files.
read -r -a engines <<<"${ENGINES:-native wasm2c-guard wasm2c-bounds cranelift wasmi boa}"
command_for() {
  local engine=$1 script=$2 mode=$3
  case "$engine" in
    native) echo "./native-probe $script $mode" ;;
    wasm2c-guard) echo "./qjs-runner-wasm2c-guard $script $mode" ;;
    wasm2c-bounds) echo "./qjs-runner-wasm2c-bounds $script --native-stack-mib 64 $mode" ;;
    # This runner reads 0 as no fuel at all, not metering off.
    cranelift) echo "./qjs-runner-pulley qjs_guest.wasm $script --native --fuel-per-turn 1000000000000 $mode" ;;
    wasmi) echo "./qjs-runner qjs_guest.wasm $script --native-stack-mib 1 --fuel-per-turn 0 $mode" ;;
    boa) echo "./qjs-runner-boa $script $mode" ;;
  esac
}

for run in 1 2 3; do
  for engine in "${engines[@]}"; do
    on_device "$(command_for "$engine" w-worker.js "--replay corpus.bin")" | grep RESULT \
      | sed "s/^RESULT /RESULT {\"run\":$run,\"thermal\":\"$(thermal)\",/; s/,{/,/" >"$out/$engine-replay-$run.txt" || true
    on_device "$(command_for "$engine" w-worker.js "--instances 10")" | grep RESULT >"$out/$engine-instances-$run.txt" || true
  done
done

for engine in "${engines[@]}"; do
  rm -f "$out/throughput-$engine.txt"
  for workload in codec rxjs pingpong-64 pingpong-4k; do
    line=$(on_device "$(command_for "$engine" "w-$workload.js" "")" | grep BENCH_RESULT || echo "no result")
    echo "$workload $engine: $line" | tee -a "$out/throughput-$engine.txt"
  done
  case "$engine" in
    wasm2c-guard | wasm2c-bounds | wasmi)
      echo "memcap $engine: $(on_device "$(command_for "$engine" t1-memory.js "--memory-mib 8")" | grep caught)" \
        | tee -a "$out/throughput-$engine.txt" ;;
  esac
done
echo "guard capacity: $(on_device "./qjs-runner-wasm2c-guard x --guard-capacity 64" | grep RESULT)" | tee "$out/guard-capacity.txt"
grep -h RESULT "$out"/*-replay-*.txt
