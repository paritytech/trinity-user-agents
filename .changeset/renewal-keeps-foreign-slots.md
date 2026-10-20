---
"@parity/truapi-host": patch
---

The signing host's statement-store renewal pass takes only free slots and never replaces a slot held by another account. When a period is full, a remaining renewal target that already holds a slot is reported `AlreadyAllocated` and the rest `SkippedExhausted`. A duplicate-submit retry respects a caller that disallows replacement.
