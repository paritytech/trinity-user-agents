---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

The core's chain client survives a refused chainHead re-follow, as dotli sends while it waits to redial a halted chain. A refusal whose `error.data` is `dotli:chain-halted` is asked again after 1, 2, 4, 8 and 16 seconds, and the core's chain reads waiting on the follow, such as building a transaction, resolving a ring, or submitting to Bulletin, wait for it within their own deadlines instead of failing with `subscription dropped`. A halt that outlasts the last pause ends the follow, and the reads waiting on it fail. Any other refusal ends the follow at once. Reading the current block for a transaction or a ring gives up after 45 seconds, connecting included, and a read that times out on an open follow ends that follow, so the node drops the operations started on it and the next read starts a new one. The error that ends the core's chain follow is logged at `warn`.
