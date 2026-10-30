---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

While dotli redials a halted chain, the core retries the requests it refuses after 1, 2, 4, 8 and 16 seconds. `createTransaction` and other chain reads wait out a short halt instead of failing with `subscription dropped`. The error that ends a chain follow is logged at `warn`.
