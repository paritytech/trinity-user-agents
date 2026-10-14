---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Anchor Bulletin preimage transaction mortality to finalized blocks so discarded forks do not invalidate signatures. Keep nonce lookup and dry-run validation at the best block.
