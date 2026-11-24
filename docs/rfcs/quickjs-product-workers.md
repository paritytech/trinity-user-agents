---
title: "QuickJS product workers"
owner: "Ruslan Rezin"
status: draft
---

# RFC — QuickJS product workers

## Summary

Native hosts run a product's worker executable in QuickJS instead of a hidden web view. On iOS, QuickJS is compiled to
WebAssembly and translated to C by wasm2c, so a fault in the engine stays inside the worker's own memory. On Android it
runs natively in an isolated process. Workers get the same small JavaScript environment on both, and products keep using
`@parity/truapi` unchanged.

## Motivation

The [Product Manifest Format](product-manifest.md) defines the worker as a product's single background process, and
[Worker Lifecycle](worker-lifecycle.md) says when it runs. On iOS and Android that process is a hidden web view today,
which fails the lifecycle in three ways:

- iOS reclaims a web view's content process when the app is in the background, so a referenced worker dies with no
  signal to the host.
- A web view costs about 17 MiB per worker, most of it in its own content process, and iOS caps how many content
  processes an app may hold.
- The host cannot cap a web view's memory or stop a script that does not return.

### Requirements

1. **Contained.** A memory-safety bug in the JavaScript engine cannot read or write host memory outside the worker.
2. **Bounded.** The host caps each worker's memory and the CPU time of each turn.
3. **Resident.** A worker runs inside the app's own process, so it lives exactly as long as the host keeps it.
4. **Uniform.** A worker gets the same JavaScript environment on every native host.
5. **Additive.** Nothing is added to the TrUAPI protocol, and products need no change.

## Approach

The design has five parts:

- **Environment**: what a worker script can use.
- **Transport**: how frames reach the core.
- **Engine**: what runs the script on each platform.
- **Limits**: how memory and time are capped.
- **Network**: how a worker reaches its own backend.

### Environment

The worker's entry module and everything it imports load as ES modules from the worker executable's directory. A
specifier resolves only to a file inside that directory, and a bare specifier fails, so a product bundles its
dependencies. The global object offers timers, `queueMicrotask`, `console`, `performance.now`, `crypto.getRandomValues`,
`TextEncoder` and `TextDecoder`, `fetch`, and the transport. The DOM, Web Storage, IndexedDB, `WebSocket`,
`XMLHttpRequest`, `EventSource` and `WebAssembly` are absent; each named global exists and throws `ApiUnavailableError`
when used, so feature detection still works. State that must survive a stop goes through host storage, as
[Worker Lifecycle](worker-lifecycle.md) requires.

### Transport

The core opens one in-process frame channel per worker execution and carries the same frames a web view exchanged. The
worker finds the channel as `__HOST_API_TRANSPORT__` on the global object, where `getClientSync()` in `@parity/truapi`
looks for it when there is no window.

### Engine

Both platforms run the same QuickJS-NG release and the same environment code, behind one engine interface in the core.

Engine per platform:

| Host    | Engine                                                                                     | Boundary                                                                                                              |
| ------- | ------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| iOS     | QuickJS compiled to `wasm32`, translated to C by wasm2c at build time, linked into the app | The guest's linear memory: every load and store is bounds-checked, and the host checks every pointer an import passes |
| Android | QuickJS compiled natively, in an `isolatedProcess` service                                 | The OS process: its own user ID, no app data and no permissions; frames cross Binder                                  |

The iOS guest is built from source with every core build. Nothing is compiled at run time, which iOS does not allow.

### Limits

A worker's memory is capped at 32 MiB; past the cap an allocation fails as a catchable out-of-memory error and the
worker keeps running. A turn is the work one inbound frame, one timer or one job queue drain causes. Each turn has a CPU
time budget, measured on the worker's thread so a suspended app does not spend it. QuickJS's interrupt handler checks
the budget; on overrun the host stops the worker and counts it as a crash.

### Network

`fetch` sends its request through an HTTP callback on the host, which performs it and returns the response. A funding
provider's worker uses it to reach its own API.

## Measurements

Every engine replayed the same 401-frame conversation, recorded from the real `@parity/truapi` client rendering the
playground's renderer tree, and returned every reply byte for byte. Medians; memory is the growth of the app's footprint
per live worker across ten.

On an iPhone XS (A12, the oldest supported iPhone), iOS 18.7:

| iPhone XS                   | QuickJS native   | wasm2c, bounds checks        | Boa                  | wasmi                   |
| --------------------------- | ---------------- | ---------------------------- | -------------------- | ----------------------- |
| Contains an engine fault    | no               | yes                          | prevents most (Rust) | yes                     |
| Render round trip p50 / p95 | 0.37 / 0.46 ms   | 0.75 / 0.79 ms               | 1.59 / 2.76 ms       | 9.1 / 9.4 ms            |
| Start to idle               | 14 ms            | 26 ms                        | 118 ms               | 203 ms                  |
| Memory per worker           | 1.9 MiB          | 3.5 MiB                      | 3.1 MiB              | 3.5 MiB                 |
| Memory cap                  | heap limit       | catchable out-of-memory      | none                 | catchable out-of-memory |
| Out-of-bounds read          | reads app memory | traps, a fresh worker starts | not testable         | not run                 |

On an Apple M5 Pro Mac, with the engines the phone run did not cover:

| Mac                         | WKWebView      | QuickJS native | wasm2c, guard pages | wasm2c, bounds checks | Cranelift      | wasmi          | Boa            |
| --------------------------- | -------------- | -------------- | ------------------- | --------------------- | -------------- | -------------- | -------------- |
| Render round trip p50 / p95 | 0.18 / 0.28 ms | 0.15 / 0.16 ms | 0.19 / 0.20 ms      | 0.31 / 0.38 ms        | 0.39 / 0.41 ms | 4.43 / 4.81 ms | 0.65 / 1.27 ms |
| Start to idle               | 54 to 115 ms   | 7.0 ms         | 7.8 ms              | 11 to 12 ms           | 13.8 ms        | 98 ms          | 55 to 69 ms    |
| Memory per worker           | 17.2 MiB       | 2.1 MiB        | 2.5 MiB             | 2.5 MiB               | 2.5 MiB        | 3.7 MiB        | 5.6 MiB        |

The harnesses, raw results and the iOS bench app are on the `spike/qjs-bench` branch under `spikes/qjs-bench/`:
`scripts/compare-wasm2c.sh` reruns the Mac table, and `scripts/build-ios.sh` with `scripts/run-ios-device.sh` the phone
table.

## Trade-offs

- iOS pays about twice native QuickJS in CPU and about 1.6 MiB more per worker for the boundary.
- On iOS the turn budget is enforced inside the sandbox, so a worker that first exploits an engine fault can disable it
  and keep a thread busy; the host can then only abandon the thread and count a crash.
- Guard pages run at about 1.3 times native but reserve 8 GiB of address space per worker, and an iPhone XS grants no
  such reservation without the Extended Virtual Addressing entitlement.
- Native QuickJS inside the iOS app is the fastest option and has no boundary.
- Boa prevents most engine faults by being written in Rust, but renders at four times and starts at eight times native
  QuickJS, and cannot cap memory or interrupt a script.
- wasmi is an audited interpreter with the same boundary, at 25 times native render time and a 203 ms start.
- Cranelift compiles wasm close to native speed, but needs a JIT, which iOS forbids.
- The iOS core build needs wasi-sdk and WABT and compiles about 13 MB of generated C.
- Workers do not run in the iOS Notification Service Extension, which shares the app's keychain group.

## Open questions

- Which origins a worker's `fetch` may reach: any HTTPS origin, or only origins its manifest declares.
