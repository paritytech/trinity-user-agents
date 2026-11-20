---
"@parity/truapi-host": patch
---

The native durability engine composes transactions: a deferred transaction can wait for other transactions on the same chain, and is built once each has taken effect at a best block. Each dependency either cascades its failure to the dependent or leaves the dependent to its policy, which sees how every dependency ended. A dependent whose attempt fails while a dependency has not taken effect, as after a reorg, waits for it again. The JavaScript API and wire protocol are unchanged.
