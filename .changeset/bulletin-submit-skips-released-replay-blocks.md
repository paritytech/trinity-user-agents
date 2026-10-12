---
"@parity/truapi-host": patch
---

A preimage submission no longer fails when the node refuses the header of one of the finalized blocks the chain follow started with. The submission skips such a block, since the head it builds on is the newest finalized block or a best block after it, and fails only if no block can be read within the 10-second connect budget.
