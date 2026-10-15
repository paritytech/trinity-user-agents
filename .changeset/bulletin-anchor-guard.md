---
"@parity/truapi-host": patch
---

Anchor a Bulletin preimage transaction's mortality at the block it is built and dry-run at when that block is older than the finalized one, so the first submission of a session no longer fails with `BadProof` or `AncientBirthBlock`.
