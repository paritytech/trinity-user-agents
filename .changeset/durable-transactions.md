---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native signing hosts follow presigned transactions to a verdict in the core. The core database gains a `durable_tx` ledger: a domain registers presigned mortal extrinsics, the core broadcasts them once committed, watches them, and decides every one it no longer watches from the chain, including those a previous process left live. `HostCallbacks` gains the required `durable_work_changed(pending)`, which the Swift and Kotlin `HostBridge` default to a no-op; while it reports `true` the host keeps a background task running that awaits `runDurableRecovery()`. No domain registers transactions yet.
