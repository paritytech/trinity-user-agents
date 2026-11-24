# qjs-bench (M0 spike)

Throwaway benchmark for running TrUAPI product workers in QuickJS compiled to
WebAssembly. It lives on the `spike/qjs-bench` branch and is never merged to
`main`. It has its own Cargo workspace, so the main workspace, its lints and CI
never see it.

## Layout

- `guest/`: rquickjs 0.14.0 built for `wasm32-wasip1`. Exports `alloc`,
  `dealloc`, `init`, `on_frame`, `on_timer` and `run_jobs`. Imports `frame_out`,
  `now_ms`, `random_fill`, `timer_set`, `timer_clear` and `log` from the
  `truapi` module. `guest/js/prelude.js` builds the worker globals from those
  imports and deletes the raw host object.
- `runner/`: a macOS host on wasmi 2.0.2. It applies fuel per turn and a
  linear-memory cap, links only the WASI calls wasi-libc needs, and echoes
  every outbound frame back to the guest.
- `runner-wasm2c/`: the same guest translated to C by wasm2c and compiled
  into the runner, with guard pages or explicit bounds checks (T6).
- `scripts/build-android.sh` and `scripts/run-android.sh`: every runner on
  an Android phone over adb (T8).
- `runner-boa/`: the same prelude and bindings on Boa, the JavaScript
  engine written in Rust, for comparison (T6).
- `benchkit/` and `ios-bench/`: the engines behind a C API, and the SwiftUI
  app that runs them on a simulator or an iPhone (T7).
- `samples/`: scripts for the T1 checks.
- `workloads/`: the bench worker, the host frames it is driven with, and
  `corpus/corpus.bin`, the recorded conversation every engine replays (T2).

## Run

```bash
scripts/build-guest.sh
cargo run -p qjs-runner --release -- target/wasm32-wasip1/release/qjs_guest.wasm samples/t1-hello.js
cargo run -p qjs-runner --release -- target/wasm32-wasip1/release/qjs_guest.wasm samples/t1-busy-loop.js --fuel-per-turn 50000000
cargo run -p qjs-runner --release -- target/wasm32-wasip1/release/qjs_guest.wasm samples/t1-memory.js --memory-mib 8
```

The workloads need the generated TS client: run `./scripts/codegen.sh` and
`npm install` in `js/packages/truapi` once, then:

```bash
cd workloads && bun install
bun run corpus   # rebuild corpus/corpus.bin
bun run verify   # T2 done-when check
```

## T3: the workload bundle

`bun run check` builds `dist/workloads.js` (the bench worker, W3 to W5 and
the client, one minified IIFE of 117 KB, as a product would ship it),
`dist/webview-shim.js` for E1, and one `dist/w-<name>.js` wrapper per
workload that sets `__BENCH` first, so an engine only ever evaluates one
file. It then runs every wrapper under Bun, one process each; the worker
replays the corpus with zero mismatches.

The native probe (`cargo run -p qjs-guest --release --bin native-probe --
workloads/dist/w-codec.js`) compiles the same guest code (prelude, bindings,
QuickJS) for the Mac instead of wasm. That is the E2 engine, early.

### Mac results

| Workload | Bun (JIT) | QuickJS native (E2) | QuickJS in wasm on wasmi (E3) |
|---|---|---|---|
| Worker bundle, evaluate to idle | 7.7 ms | 6.9 ms | 97 to 117 ms |
| W3 codec, render round trips/s | 33 840 | 4 597 | 149 (6.7 ms each) |
| W4 rxjs, events/s | 2 057 000 | 304 652 | 9 776 |
| W5 frame ping-pong, 64 B and 4 KB, p50 | under 1 µs | 1 µs | 41 to 43 µs |

- QuickJS itself is about 7 times slower than a JIT, which is what an
  interpreter costs.
- JavaScriptCore without its JIT, which is what `JSContext` gets inside an
  iOS app, runs at the same speed as native QuickJS on the codec (4 493
  round trips/s, with a JS text-codec shim) and 1.7 times faster on rxjs
  (531 000 events/s). Measured with macOS's shell:
  `jsc --useJIT=false src/jsc-shim.js dist/w-codec.js`. With the JIT it
  reaches 22 216 and 1 890 000. So only WKWebView's own process gets the
  fast JavaScriptCore; in-app it is in QuickJS's league.
- The wasm layer on wasmi costs a further factor of about 30, against the
  plan's estimate of 3 to 10. This, not QuickJS, is what puts the gate at
  risk on iOS, where no JIT is allowed.
- The host boundary is cheap: 41 µs per round trip including the JS promise
  machinery, flat between 64 B and 4 KB.
- Building the guest (QuickJS's C code included) at `opt-level = 3` instead
  of `"s"` gains about 9% and grows the module from 882 KB to 1.29 MB, so the
  guest stays at `"s"`.

### Finding: wasmi needs `opt-level = 3` or its dispatch grows the native stack

At the workspace's `opt-level = "s"`, evaluating the 117 KB bundle overflows
a 16 MiB native thread stack (it needs between 16 and 32 MiB), while the
tiny T1 scripts never did. wasmi 2.0 dispatches instructions with tail calls
and its build script enables that at `"s"`, but LLVM does not reliably turn
them into jumps at that level, so the stack grows with every instruction
executed. Evidence:

- Eager compilation succeeds on 8 MiB, so translation is not the cause.
- With wasmi's `portable-dispatch` feature the bundle runs on 1 MiB, but
  evaluation takes 250 ms instead of 117 ms.
- With tail-call dispatch and wasmi alone at `opt-level = 3` (the profile
  overrides in `Cargo.toml`), the bundle runs on 1 MiB and evaluation takes
  97 ms.

The core's release profile uses `opt-level = "z"`, so phase 2 must give the
wasmi crates `opt-level = 3`. The behaviour looks worth reporting to wasmi.

## Engine comparison on the Mac, web view included

`scripts/compare-mac.sh` runs four engines three times each on the worker
bundle and the corpus; raw lines are in `results/mac/`. The web view is a
real WKWebView (`webview-mac/WebViewBench.swift`), set up as
`WKWebViewJSEngine` does on iOS, with JavaScript in a separate WebContent
process with the JIT. Medians of three runs:

| Mac | WKWebView | QuickJS native | wasm, Cranelift (Android) | wasm, wasmi (iOS) |
|---|---|---|---|---|
| Corpus replay, 401 steps | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches |
| Render round trip p50 / p95 | 0.18 / 0.28 ms | 0.15 / 0.20 ms | 0.39 / 0.41 ms | 4.44 / 4.90 ms |
| First boot to idle | 115 ms | 7.3 ms | 14.0 ms | 99.7 ms |
| Later boots to idle, p50 | 54 ms | 6.6 ms | 13.6 ms | 94.3 ms |
| Memory per instance | 17.2 MiB (15.2 in its WebContent process) | 2.1 MiB | 2.5 MiB | 3.7 MiB |
| Once per process | | | 26.6 MiB and 550 ms to compile | 1.5 MiB and 3 ms to compile |
| W3 codec, round trips/s | 29 412 | 4 597 | 1 772 | 149 |

- Memory per instance is the growth of the app's physical footprint across
  10 live instances; for the web view it adds each instance's own
  WebContent process.
- The web view's render round trip is mostly the trip to its process and
  back, which is why native QuickJS beats it on this small render.
- Cranelift's compile cost moves to build time with a precompiled module.

## T5: wasmtime, Pulley and Cranelift

`runner-pulley/` runs the same guest, imports, WASI set, fuel and memory
cap on wasmtime 51. By default it compiles to Pulley bytecode
(`Config::target("pulley64")`), wasmtime's interpreter and the only wasmtime
backend iOS allows. `--native` compiles for the host CPU with Cranelift
instead, which is what Android could use.

| Mac | QuickJS native | wasm, Cranelift JIT | wasm, wasmi | wasm, Pulley |
|---|---|---|---|---|
| Worker bundle, evaluate to idle | 6.9 ms | 13.8 ms | 97 ms | 341 ms |
| W3 codec, round trips/s | 4 597 | 1 772 | 149 | 51 |
| W4 rxjs, events/s | 304 652 | 119 352 | 9 776 | 3 493 |
| W5 ping-pong 64 B, p50 | 1 µs | | 41 µs | 110 µs |
| Module compile at startup | | 535 ms | 3 ms (lazy) | 588 ms |

- Pulley is about three times slower than wasmi on every workload, so wasmi
  stays the iOS engine and E4 drops out of the device runs.
- Cranelift runs the wasm about 2.6 times slower than native QuickJS, the
  cost of the sandbox's checks and of losing computed-goto dispatch. On
  Android, where JIT is allowed, that is a usable engine. Its 535 ms compile
  would move to build time with `Engine::precompile_module`.
- wasmi is about 12 times slower than Cranelift, the usual gap between an
  interpreter and a JIT. Together that gives the ~30 times between native
  QuickJS and wasmi; nothing in the guest build looks pathological.
- Fuel metering costs nothing measurable in wasmi (151 against 149 codec
  round trips per second with `--fuel-per-turn 0`), so fuel stays the way to
  stop runaway scripts.

## T6: wasm2c, the sandbox compiled ahead of time

`runner-wasm2c/` translates the same guest module to C with wasm2c (WABT
1.0.42, `brew install wabt`), compiles it at `-O2` with the wasm2c runtime
and a small import glue (`c/glue.c`), and links it into the runner. Nothing
is generated or interpreted at run time, so this is a form iOS allows. The
generated C keeps the wasm rules: every load and store is checked against
the guest's linear memory, indirect calls are type-checked, and the guest
reaches the host only through its imports, whose pointers the glue checks
before the host reads them. Two variants, chosen at build time with
`QJS_W2C_MEMCHECK`:

- `guard`: guard pages and a signal handler catch out-of-bounds accesses.
  Each instance reserves about 8 GiB of address space.
- `bounds`: an explicit check on every access and a call-depth counter, no
  address-space reservation (built with `WASM_RT_USE_MMAP=0`; see the
  iPhone XS results under T7).

`scripts/compare-wasm2c.sh` runs both beside native QuickJS, wasmi and Boa;
raw lines are in `results/mac-wasm2c/`. Medians of three runs on the same
Mac; Cranelift is from the T5 runs. Boa (`runner-boa/`, Boa 0.22 with
Temporal and Intl off) runs the same prelude and bindings natively, with no
sandbox: it is the memory-safe-language option rather than a contained one.
The raw files are from the later run that added Boa. That run was noisier
for the other engines (native p95 0.32 ms, bounds-check boot up to 17 ms), so
their columns keep the first run's medians; the ratios between engines held.

| Mac | QuickJS native | wasm2c, guard pages | wasm2c, bounds checks | wasm, Cranelift | wasm, wasmi | Boa |
|---|---|---|---|---|---|---|
| Corpus replay, 401 steps | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches |
| Render round trip p50 / p95 | 0.15 / 0.16 ms | 0.19 / 0.20 ms | 0.31 / 0.38 ms | 0.39 / 0.41 ms | 4.43 / 4.81 ms | 0.65 / 1.27 ms |
| Worker bundle, boot to idle | 7.0 ms | 7.8 ms | 11 to 12 ms | 13.8 ms | 98 ms | 55 to 69 ms |
| Memory per instance (10 live) | 2.1 MiB | 2.5 MiB | 2.5 MiB | 2.5 MiB | 3.7 MiB | 5.6 MiB |
| W3 codec, round trips/s | 4 558 | 3 585 | 2 204 | 1 772 | 154 | 841 to 874 |
| W4 rxjs, events/s | 303 986 | 226 398 | 151 721 | 119 352 | 9 497 | 57 907 to 60 912 |
| W5 ping-pong 64 B / 4 KB, p50 | 0.9 / 1.0 µs | 2.4 / 2.6 µs | 3.4 / 3.6 µs | | 40 / 43 µs | 2.9 / 4.0 µs |
| Runner binary, stripped | 1.07 MB | 1.56 MB | 2.40 MB | | 1.71 MB + 0.88 MB guest | 4.09 MB |
| Contains an engine bug | no | yes | yes | yes | yes | no (Rust prevents most) |
| Heap cap and turn budget | yes | cap; budget via interrupt | cap; budget via interrupt | fuel and cap | fuel and cap | neither |

- The sandbox costs about 1.3 times native with guard pages and about 2
  times with explicit bounds checks. Both beat Cranelift, because clang
  optimises the generated C harder than Cranelift compiles wasm, and both
  are 15 to 30 times faster than wasmi.
- The memory cap works as on wasmi: the host lowers the guest memory's
  maximum, and with 8 MiB the script gets a catchable `InternalError: out
  of memory` after 6 MiB and keeps running.
- There is no fuel. wasm2c has no instruction metering, so a runaway script
  is not stopped by this runner. The fix belongs in the guest: QuickJS's
  interrupt handler, which the native engine already uses, calling a
  `should_interrupt` import that checks the host's deadline.
- Guard pages need address space iOS may not give: about 8 GiB per
  instance, against a per-app limit that depends on the device unless the
  app has the extended virtual addressing entitlement. The bounds-check
  variant needs no reservation, so it is the safe default until a device
  run shows what guard pages allow.
- Compiling the 13 MB of generated C takes about 10 s per variant here.

## T7: the iOS bench app

`benchkit/` is a static library with a three-function C API
(`benchkit_run`, `benchkit_free`, `benchkit_variant`) over four engines:
native QuickJS, the wasm guest on wasmi, the wasm guest through wasm2c, and
Boa.
`ios-bench/` is a SwiftUI app that links it as `build/BenchKit.xcframework`,
bundles the workloads, runs a suite on launch and prints one JSON line per
measurement (`BENCH {…}`), also appended to `Documents/results.jsonl`. The
runners are libraries now, so the Mac binaries and the app share the same
measurement code.

```bash
scripts/build-ios.sh bounds sim        # or guard; wasm2c's runtime is built for one
scripts/run-ios-sim.sh bounds quick    # results/ios-sim/bounds.jsonl
scripts/build-ios.sh bounds device <team-id>
```

Launch arguments: `--suite quick|full`, `--runs N`, `--exit`. The quick suite
replays the corpus three times per engine, boots 10 live workers per engine,
checks the 8 MiB memory cap on both wasm engines, makes the wasm2c guest
read out of bounds (`oob-trap`), probes how many 8 GiB guard-page
reservations fit (`guard-capacity`), and runs the codec workload on native,
wasm2c and Boa. `full` adds rxjs, ping-pong and wasmi throughput.

Build notes:

- BenchKit is a `staticlib` only. Built as `staticlib` and `rlib` together
  with LTO, rustc writes LLVM bitcode that Xcode's older LLVM cannot read.
- The workspace profile strips symbols, which on a static library removes
  the C entry points, so the script builds with `CARGO_PROFILE_RELEASE_STRIP=false`.
- rquickjs uses bindgen on iOS (`guest/Cargo.toml`); Xcode supplies libclang.
- The wasm2c variants cannot share one binary, since the wasm2c runtime is
  compiled for guard pages or bounds checks, so each variant is its own build.
- A thread that ran a wasm2c guest calls `release_thread` before it exits.
  Rust replaces the thread's alternate signal stack during its own teardown,
  so a thread-local destructor would free wasm2c's too late.

### Simulator results (iPhone 17 Pro simulator, iOS 26.5, on the Mac)

Every step passed in both variants. Medians of three replays:

| Simulator | QuickJS native | wasm2c, guard pages | wasm2c, bounds checks | wasm, wasmi | Boa |
|---|---|---|---|---|---|
| Corpus replay, 401 steps | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches |
| Render round trip p50 / p95 | 0.15 to 0.16 / 0.26 ms | 0.20 / 0.24 ms | 0.31 / 0.39 ms | 4.4 / 4.5 ms | 0.65 / 1.29 ms |
| Boot to idle, 10 workers, p50 | 7.1 to 7.4 ms | 8.0 ms | 10.9 ms | 94 to 97 ms | 57 ms |
| Memory per worker | 1.9 MiB | 2.5 MiB | 2.5 MiB | 3.6 to 4.0 MiB | 3.1 MiB (5.6 on the Mac) |
| Codec, round trips/s | 4 342 to 4 398 | 3 427 | 2 191 | | 836 |
| 8 MiB cap | | catchable OOM after 6 MiB | catchable OOM after 6 MiB | catchable OOM after 6 MiB | no heap limit to set |
| Out-of-bounds read | | trapped, fresh guest boots | trapped, fresh guest boots | | |
| 8 GiB reservations that fit | | 64 of 64 | 64 of 64 | | |

- The ratios match the Mac's, as they should: the simulator runs on the
  Mac's CPU. They say nothing about iPhone speed.
- The guard-page trap goes through wasm2c's signal handler on the
  alternate signal stack under the iOS runtime and comes back as a clean
  trap; on Apple platforms wasm2c reports it as "out of bounds, or call stack
  exhausted".
- The guard capacity is the Mac kernel's. iOS limits an app's address space,
  so that row is the first thing the device runs answer, with and without the
  extended virtual addressing entitlement.
- The device app (`build-ios.sh … device`, unsigned) compiles; its binary is
  5.0 MB with native QuickJS, wasmi and wasm2c. Boa adds about 3.3 MB to the
  simulator binary.
- Boa's memory per worker moves more between runs than the other engines'
  (3.1 MiB here, 5.6 MiB on the Mac), likely because its garbage collector
  runs at different points; the device runs will show its spread.

### iPhone XS (A12, iOS 18.7)

`scripts/run-ios-device.sh <udid> <variant> [quick|full] [label]` installs
the app built by `build-ios.sh <variant> device <team>`, runs it with the
console attached and keeps the lines in `results/ios-device/`. Two runs of the
bounds-check build (`run1` quick, `run2` full), medians over six replays.
Thermal state stayed nominal through the replays and reached fair during the
long throughput workloads.

| iPhone XS | QuickJS native | wasm2c, bounds checks | Boa | wasm, wasmi |
|---|---|---|---|---|
| Corpus replay, 401 steps | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches |
| Render round trip p50 / p95 | 0.37 / 0.46 ms | 0.75 / 0.79 ms | 1.59 / 2.76 ms | 9.1 / 9.4 ms |
| Boot to idle, 10 workers, p50 | 14.1 ms | 25.7 ms | 117 to 120 ms | 203 ms |
| Memory per worker | 1.9 MiB | 3.5 MiB | 2.9 to 3.2 MiB | 3.5 MiB |
| Codec, round trips/s | 1 613 to 1 912 | 839 to 919 | 310 to 344 | 70 |
| rxjs, events/s | 92 724 | 49 499 | 17 444 | 4 102 |
| Ping-pong 64 B / 4 KB, p50 | 2.8 / 3.0 µs | 8.7 / 9.1 µs | 8.9 / 11.6 µs | 100 / 103 µs |
| 8 MiB cap | | catchable OOM after 6 MiB | no heap limit to set | catchable OOM after 6 MiB |
| Out-of-bounds read | | trapped, fresh guest boots | | |

- wasm2c with bounds checks costs about 2 times native on the A12 as on the
  Mac: 0.75 ms a render, well inside a 16 ms frame, and 26 ms to boot. Boa is
  about 4 times native on renders and 8 times on boot; wasmi about 25 times
  on renders, half the 18 to 22 ms the M0 estimate feared.
- **Guard pages do not fit.** Not one 8 GiB reservation succeeds on this
  phone (`guard-capacity`: 0 of 64), so the guard-page build cannot start a
  single worker; it now reports that as an error instead of letting the
  runtime abort the app. Whether the extended virtual addressing
  entitlement changes this is still to measure.
- **The bounds-check build needed no reservation either.** wasm2c's runtime
  reserves the module's maximum memory up front even with bounds checks: our
  guest declares none, so 4 GiB per instance, which iOS granted once and then
  refused, aborting the app. The bounds variant now builds with
  `WASM_RT_USE_MMAP=0`, so memory comes from the allocator and grows by
  `realloc`. That costs about 1 MiB more per worker here (3.5 MiB against
  2.5 MiB with mmap on the Mac), which a guest that declares its maximum, or
  a smaller initial heap, would win back.
- The app binary with all four engines is 11.7 MB, unstripped.

## T8: Android

`scripts/build-android.sh` cross-compiles every runner for 64-bit ARM at
API 29 (the app's minSdk) with cargo-ndk; `scripts/run-android.sh` pushes
them to `/data/local/tmp` with the workloads and runs the same comparison
from `adb shell` into `results/android/<model>/`. Memory is the proportional
set size from `/proc/self/smaps_rollup`, which Android's memory accounting
uses. The runners are plain processes, not an app or an isolated service, so
the numbers are engine speed and memory without Binder in the way.

Samsung Galaxy A26 5G (SM-A266B, Exynos 1380, 8 cores), Android 16,
three runs, medians:

| Galaxy A26 | QuickJS native | wasm2c, guard pages | wasm2c, bounds checks | wasm, Cranelift | wasm, wasmi | Boa |
|---|---|---|---|---|---|---|
| Corpus replay, 401 steps | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches | 0 mismatches |
| Render round trip p50 / p95 | 0.64 / 0.73 ms | 0.90 / 1.00 ms | 1.49 / 1.67 ms | 1.21 / 1.26 ms | 11.7 / 12.0 ms | 4.16 / 11.3 ms |
| Boot to idle, 10 workers, p50 | 22 to 28 ms | 27 to 31 ms | 84 to 104 ms | 39 to 47 ms | 292 to 308 ms | 214 to 233 ms |
| Memory per worker | 2.0 MiB | 2.4 MiB | 3.5 MiB | 2.4 MiB | 3.6 MiB | 6.2 to 6.4 MiB |
| Codec, round trips/s | 1 119 | 847 | 526 | 587 | 67 | 106 |
| rxjs, events/s | 75 534 | 58 075 | 36 824 | 37 815 | 4 161 | 7 079 |
| Ping-pong 64 B / 4 KB, p50 | 4.2 / 4.6 µs | 6.4 / 6.6 µs | 10.7 / 11.5 µs | 15.3 / 16.1 µs | 86 / 92 µs | 23 / 27 µs |
| Once per process | | | | 2.3 s and 30 MiB to compile | | |
| 8 MiB cap | | catchable OOM after 6 MiB | catchable OOM after 6 MiB | | catchable OOM after 6 MiB | no heap limit to set |

- **Throttled.** The thermal status read moderate (2) after every replay,
  with the battery at 28% and a fast charger cycling; at rest it reads 0. The
  absolute numbers are pessimistic; the ratios between engines are the
  useful part. A rerun on a cool phone, unplugged above half charge, would
  firm them up.
- **Native QuickJS**, the Android engine, renders in 0.64 ms and boots in
  about 26 ms: well inside the budgets, with the isolated process still to
  add a Binder hop per frame.
- **Guard pages fit here:** 61 of 64 8 GiB reservations succeed, so on
  Android wasm2c with guard pages runs at 1.4 times native.
- **wasm2c with bounds checks boots slowly on Android**, at about 4 times
  native against 1.8 times on the iPhone XS. Without mmap the guest's memory
  grows by `realloc` from its initial size, and every growth copies. A guest
  that declares its 32 MiB maximum lets the runtime reserve just that much
  with mmap and grow in place, which removes the copies on both platforms
  without the 4 GiB reservation iOS refused.
- **Cranelift's JIT runs from `adb shell`**, but costs 2.3 s and 30 MiB to
  compile the module, which only precompiling at build time avoids.
- **Boa needs its enum value representation on Android.** Its default NaN
  boxing panics on start: Android's ARM64 pointers carry a tag in the top
  byte. With `jsvalue-enum` it runs, at 6.5 times native on renders.

## T2: the corpus

The M0 plan proposed recording a Pocket battery run. That does not work as a
replay source, for three reasons:

- In the Pocket phase the worker is `pocket-battery.ts` itself, so a
  recording only replays against that script, not the playground worker.
- The CLI never sends render requests, so the gate's render round trip never
  appears in any recording.
- Every battery script logs in first, which needs a signing session and the
  network, so recordings would not be repeatable.

The corpus is built from the protocol itself instead:

- `src/bench-worker.ts` runs the real `@parity/truapi` client and transport,
  and answers host-initiated render requests with the playground's real
  `ChatDiagnosis.rendererNode()` tree. The worker glue is ours because the
  playground worker ties each render to a random run id and to a message id
  it only learns from a live chat host.
- `src/host-frames.ts` encodes host frames with the client's generated codecs,
  exactly as the core sends them: a handshake ping, then 200 V1 render requests
  for chat messages (`h:1` to `h:200`, the core's host-initiated id scheme),
  each closed by a Stop.
- `src/record-corpus.ts` replays those frames against the worker under Bun and
  records each reply into `corpus/corpus.bin`. `src/corpus-format.ts`
  describes the format.

Result: 401 inbound frames, 201 outbound frames (1 handshake answer, 200
renderer trees, the largest 519 bytes), 124 KB. `bun run verify` replays the
corpus again, checks every outbound frame byte for byte, and decodes each one
with the generated wire decoder.

### Finding: views with a byte offset break frame decoding

`decodeWireMessage` fails on a `Uint8Array` that is a view with a non-zero
`byteOffset`. It throws `RangeError: Length out of range of buffer` or
decodes the wrong bytes, because `scale-ts`'s string decoder builds its
`DataView` from `bytes.buffer` at its own position and ignores the view's
offset. Browsers, Bun and the QuickJS guest hand over fresh buffers, so
today's hosts do not hit this. A Node `Buffer` slice from a pooled
allocation would. Repro: encode a frame, copy it into a larger array at
offset 16, and decode `array.subarray(16)`.

## T1 results (Apple silicon Mac, release build)

| Check | Result |
|---|---|
| rquickjs 0.14.0 on `wasm32-wasip1` | Builds with no setup beyond `rustup target add`. rquickjs-sys ships prebuilt bindings for the target and downloads wasi-sdk 24 itself. |
| Guest size | 882 KB; 781 KB after `wasm-opt -Oz`; 333 KB gzipped |
| Compile (wasmi, lazy) | about 4 ms |
| Instantiate | about 50 µs |
| `init` to idle (prelude and script) | about 8 to 10 ms |
| Linear memory after `t1-hello.js` | 1.4 MiB |
| Timer, interval, microtask, frame echo, UTF-8, random | all pass in `t1-hello.js`; `__host` is not reachable from the script |
| Endless loop | stopped by the host: "all fuel consumed by WebAssembly" |
| QuickJS heap limit (24 MiB) | JS gets a catchable `InternalError: out of memory` after 23 MiB, and the guest keeps running |
| Host memory cap below the heap limit (8 MiB) | `memory.grow` is refused, `malloc` fails, and JS gets the same catchable error after 6 MiB |
| WASI imports the guest declares | `environ_get`, `environ_sizes_get`, `clock_time_get`, `fd_close`, `fd_fdstat_get`, `fd_prestat_get`, `fd_prestat_dir_name`, `fd_seek`, `fd_write`, `proc_exit`. The `fd_prestat_*` calls (file-system preopens) are declared but never called; any call to an import the host does not implement traps with its name. |

### Findings for phase 2

- wasmi charges bulk memory operations by the byte, so fuel does not track
  time: `init` costs between 10 and 100 million units of fuel but takes about
  8 ms. The resource policy's CPU limits have to be set by calibrating fuel
  against wall-clock time on the device. wasmi's `Config::fuel_cost` and
  `Config::operator_cost` make the costs tunable.
- A heap close to its limit burns fuel in garbage collection. Filling ordinary
  arrays element by element ran out of fuel before reaching the heap limit,
  while typed arrays hit the limit in 3 ms.
- Startup is dominated by evaluating the prelude and the script, not by wasmi.
  The bytecode cache in the plan targets the right cost.
