---
"@parity/truapi-host": major
---

The wasm core fetches newer copies of its Trinity stack descriptor while it runs. Breaking for a host that uses the raw
glue: `WasmPairingHostRuntime`, `WasmSigningHostRuntime` and `WasmProductRuntime` have no constructor. Build each with
`await WasmPairingHostRuntime.create(callbacks, hostConfig)` instead, which resolves once the core has read its stack
record from the host's core storage, or after a few seconds without it. Hosts built with
`createWebWorkerPairingHostRuntime` need no change.

A host passes two raw callbacks to `create`, typed as `StackCallbacks`: `fetchStackDescriptor` makes the HTTP request,
and `stackChanged` receives a `StackStatus` at start and on each change. The core asks for a fetch once its copy is an
hour old, which is at start unless its stored copy was fetched less than an hour ago, and on `refreshStack()`, and
`stackStatus()` returns the current status. A `WasmProductRuntime` built on its own does not refresh. The core keeps the
last copy it verified in core storage under the new `StackRecord` key. A newer copy that changes a genesis hash, the
network, the TLD or the generation sets `restartRequired` once it is stored, and applies once the runtime is built
again. `StackStatus.generation` names the generation the core runs, for a host to key cached chain data by. Without
`fetchStackDescriptor` the core stays on the copy it started from. The `/web` worker runtime does not pass these
callbacks yet.
