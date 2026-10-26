---
"@parity/truapi-host": major
---

Host integrations implement `requestWorkerTransition(productId, transition)` for worker lifecycle requests. `Start` requires the worker to run; `Stop` allows it to stop. Native Rust integrations use `request_worker_transition`, and the ledger controller is `WorkerController` with `install_worker_controller`.
