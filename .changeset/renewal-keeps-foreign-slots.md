---
"@parity/truapi-host": patch
"@parity/truapi": patch
---

The signing host's statement-store renewal pass takes only free slots and never replaces a slot held by another account. When a period is full, a remaining renewal target that already holds a slot is reported `AlreadyAllocated` and the rest `SkippedExhausted`. The pass renews the wallet and device targets first and rotates the order of products each period. A duplicate-submit retry respects a caller that disallows replacement, and moves to another collection with room when its own fills. The `truapi-host` CLI's `alloc-check` and docs follow the same rule.
