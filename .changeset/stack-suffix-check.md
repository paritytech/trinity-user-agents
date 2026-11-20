---
"@parity/truapi-host": minor
"@parity/truapi": patch
---

`StackStatus.mismatches` can list People's `NetworkSuffix` as `networkSuffix` when it is not the descriptor's `tld`. A
signing host on a bundled stack reports it from the suffix its allowance work reads, and keeps using People. A pairing
host never lists it.
