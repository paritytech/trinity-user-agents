---
"@parity/truapi-host": major
---

Use `const claim = runtime.claimWorker(productId)` when a feature needs a worker and call `claim.release()` when
that work ends. This replaces the host's paired `acquireWorker` and `releaseWorker` methods. Opening a render stream
no longer takes a reference. Hosts that previously relied on rendering to keep a worker running must hold a claim for
the lifetime of the surface, including across render retries.
