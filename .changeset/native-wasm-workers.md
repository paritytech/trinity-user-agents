---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Experimental: a native host runs a product worker compiled to wasm with `NativeProductExecution.start_wasm_worker`, in place of the loopback bridge. The worker gets the chat connection, the renderer connection and the permissions of its Worker execution, its log lines reach `on_core_log` with the marker `wasm-worker`, and `NativeWasmWorkerObserver` receives its end. `stop_ws_bridge` and `shutdown` stop it. Native builds now carry the `wasm-worker` feature by default.
