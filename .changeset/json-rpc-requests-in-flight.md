---
"@parity/truapi": patch
---

The core sends at most 512 requests at a time on each chain connection and holds the rest until earlier ones are answered, so a burst of core reads waits instead of failing every request on the connection.
