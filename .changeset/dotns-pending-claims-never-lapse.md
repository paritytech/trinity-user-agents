---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A paired session resolves its dotNS username again for accounts onboarded more than a week ago. Pending gateway claims count however old they are, because the PoP controller no longer expires them.
