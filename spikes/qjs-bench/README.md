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

## Run

```bash
scripts/build-guest.sh
cargo run -p qjs-runner --release -- target/wasm32-wasip1/release/qjs_guest.wasm samples/t1-hello.js
cargo run -p qjs-runner --release -- target/wasm32-wasip1/release/qjs_guest.wasm samples/t1-busy-loop.js --fuel-per-turn 50000000
cargo run -p qjs-runner --release -- target/wasm32-wasip1/release/qjs_guest.wasm samples/t1-memory.js --memory-mib 8
```

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
