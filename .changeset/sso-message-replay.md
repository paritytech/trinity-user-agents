---
"@parity/truapi-host": patch
---

Prevent repeated SSO approvals when a peer resends messages in a new envelope. SQLite replay checks wait for started writes even when their original callers are cancelled.
