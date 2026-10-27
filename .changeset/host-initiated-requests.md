---
"@parity/truapi": major
"@parity/truapi-host": patch
---

Host-initiated request methods generate asynchronous Rust callers and TypeScript handlers returning typed `neverthrow`
results. Calls support cooperative cancellation, startup buffering, and connection cleanup using the existing wire
format.

Custom `TrUApiTransport` implementations must provide `registerHostInitiatedRequest`. Transports created with
`createTransport` already implement it.
