---
"@parity/truapi-host": minor
"@parity/truapi": patch
---

People's `NetworkSuffix` should be the same as the `tld` in the stack's descriptor. When a signing host reads a
different suffix during its allowance work, `StackStatus.mismatches` now lists it as `networkSuffix`. It's only a
report: the core keeps using People. A pairing host doesn't read the suffix, so it never lists one.
