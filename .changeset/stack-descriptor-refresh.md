---
"@parity/truapi-host": minor
---

The wasm core fetches newer copies of its Trinity stack descriptor while it runs. A host passes three raw callbacks to
`WasmPairingHostRuntime` or `WasmSigningHostRuntime`, typed as `StackCallbacks`: `fetchStackDescriptor` makes the HTTP
request, `stackChanged` receives a `StackStatus` at start and on each change, and `storeStackRecord` receives the record
to pass back as `ProductRuntimeConfig.stack.stored` at the next start. The core asks for a fetch at start, once its copy
is an hour old, and on `refreshStack()`, and `stackStatus()` returns the current status. A newer copy that changes a
genesis hash, the network or the TLD sets `restartRequired` and applies once the runtime is built again. Without
`fetchStackDescriptor` the core stays on the copy it started from. The `/web` worker runtime does not pass these
callbacks yet.
