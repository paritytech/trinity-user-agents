---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A pairing host sends one `ProductSubtreeRequest` to the paired signing host per SSO session and product while one is in flight, and every caller waiting for that subtree takes its answer: `account.get_account`, statement-store and AutoSigning allocation, and the host's own subtree lookup. A request made after it settles asks again, so a failed fetch is not remembered. A cancelled caller leaves the others waiting.
