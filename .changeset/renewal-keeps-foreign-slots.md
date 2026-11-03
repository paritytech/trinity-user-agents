---
"@parity/truapi-host": patch
"@parity/truapi": patch
---

The signing host's statement-store renewal pass takes only free slots and never replaces a slot held by another account. When a period is full, a remaining renewal target that already holds a slot is reported `AlreadyAllocated` and the rest `SkippedExhausted`. The pass renews the wallet and device targets first, then the products that have gone longest without a slot, and reports outcomes in ledger order. The renewal ledger records each product's last renewed period after its target list, so an existing ledger still reads. A duplicate-submit retry respects a caller that disallows replacement, and moves to another collection with room when its own fills. The `truapi-host` CLI's signing host follows the same rule, and `alloc-check --submit` remains a diagnostic that replaces the oldest replaceable slot.
