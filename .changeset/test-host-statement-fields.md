---
"@parity/truapi-host": minor
---

`injectStatement` takes a statement's `channel`, `expiry` and `decryptionKey`, and the entries `getStatements` and `getSubmittedStatements` return carry them, so a suite can inject a statement with those fields and assert on the ones a product submits. `expiry` is a bigint, since the store packs a timestamp into its high 32 bits.
