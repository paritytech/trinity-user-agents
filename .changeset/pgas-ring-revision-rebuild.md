---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A PGAS allowance claim waits up to 90 s for Asset Hub to import its ring revision. When Asset Hub has pruned that revision, the claim locates the ring again at People's best block and rebuilds the proof once before reporting `NotAvailable`.
