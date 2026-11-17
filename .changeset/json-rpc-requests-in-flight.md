---
"@parity/truapi": patch
---

The core sends at most 512 reads at a time on each chain connection and holds the rest until earlier ones are answered, so a burst of core reads waits instead of failing every request on the connection. A chain send the host rejects closes the connection, so its callers fail instead of hanging.
