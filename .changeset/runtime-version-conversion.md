---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Preserve the requested protocol version for all runtime responses, subscription items, and domain errors, including chat message failures and subscription interruptions, through one shared conversion boundary.
