---
"@parity/truapi-host": minor
---

Add `@parity/truapi-host/node`, the pairing host for a Node process such as a CLI. `createNodePairingHostRuntime` takes the same callbacks and options as `createWebWorkerPairingHostRuntime` and returns the same runtime, with the core in a `node:worker_threads` thread and its WASM read from disk. `headlessHostCallbacks` supplies the capabilities a host without a screen does not have. The core now reads its ring-VRF module from disk when it is loaded from a `file:` URL, so no `fetch` override is needed under Node.
