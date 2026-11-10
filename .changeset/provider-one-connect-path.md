---
"@parity/truapi-provider": patch
---

A parachain connect that fails keeps the stored database of its relay for the next attempt, instead of dropping it and
leaving the relay to sync from its checkpoint.
