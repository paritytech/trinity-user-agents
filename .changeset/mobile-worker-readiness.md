---
"@parity/truapi-host": minor
---

Android and iOS can observe the current worker execution as it starts, reconnects or retries. Rust publishes each WebView execution after its module starts and its connection authenticates.

Rust implementations of `WorkerController` implement `start_worker(product_id, attempt_id)` and `stop_worker(product_id)` in place of `request_worker_transition`.
