---
"@parity/truapi": patch
---

Stop a product execution's chain close invalidating connects on the shared event bus. The close is offered to both buses and only the one holding the connection reacts, so a routine miss is no longer recorded as a close that raced a connect.
