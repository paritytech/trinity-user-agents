---
"@parity/truapi-host": patch
---

Build and dry-run a Bulletin preimage transaction at a best block no older than the finalized block its mortality is anchored to, so the first submission of a session no longer fails with `BadProof` or `AncientBirthBlock`, or reads a stale nonce.
