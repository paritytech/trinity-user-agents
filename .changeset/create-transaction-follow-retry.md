---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

The core's chain client survives a refused chainHead re-follow, as a host sends while it waits to redial a halted chain. It asks for the follow again after 1, 2, 4, 8 and 16 seconds, and calls waiting on the follow, such as building a transaction, wait for it instead of failing with `subscription dropped`. A follow still refused after the last pause ends, and the calls waiting on it fail. A provider that answers the follow with "method not found" or "invalid params" cannot serve it at all and is not asked again. The error that ends the core's chain follow is logged at `warn`.
