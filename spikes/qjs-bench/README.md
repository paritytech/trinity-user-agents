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
