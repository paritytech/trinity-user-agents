---
"@parity/truapi-host": minor
---

`injectStatement` takes a statement's `channel`, `expiry` and `decryptionKey`, and the entries `getStatements` and `getSubmittedStatements` return carry them, so a suite can inject a statement with those fields and assert on the ones a product submits. `expiry` is a bigint, since the store packs a timestamp into its high 32 bits. A topic, channel or key that is not 32 bytes, and an expiry outside a `u64` or given as an unsafe integer, is refused rather than encoded into a statement the store would misread.
