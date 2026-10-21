---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Durable transactions can be scheduled before they are signed. A domain registers a `SubmissionPolicy`; a scheduled transaction waits in the ledger until the core's submission executor asks that policy to build it, then is broadcast and followed like a presigned one. A presigned transaction may name a policy too. When a transaction with a policy is proven failed, the policy decides whether it is built again instead of failing. `durable_work_changed(true)` also covers transactions waiting to be built, so the host keeps recovery running until they are. No domain schedules transactions yet.
