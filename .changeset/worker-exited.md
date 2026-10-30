---
"@parity/truapi": minor
---

A host reports a product worker that stopped without being asked, because it crashed or the system killed it, with
`worker_exited` (`workerExited` on the web). While anything still holds that worker, the core requests `Start` again
through `request_worker_transition`, and hands a funding provider's restarted worker every session it serves once it
connects.
