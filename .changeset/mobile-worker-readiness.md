---
"@parity/truapi-host": minor
---

Native host SDK breaking change: open a product worker through the startup attempt supplied to the WebView host. Report module start and failure on that attempt. Hosts that previously handled `requestWorkerTransition`, called `openProductExecution` with `Worker`, or reported `workerExited(productId)` must adopt this API. Hosts serving only app or widget executions can omit the WebView host.

The web runtime does not export `workerExited` and does not read a `workerCrashing` callback.

Android and iOS can observe the execution as it starts, reconnects or retries. Rust publishes it after the module starts and its bridge authenticates.

Rust implementations of `WorkerController` implement `start_worker(product_id, attempt_id)` and `stop_worker(product_id)` in place of `request_worker_transition`.
