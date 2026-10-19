---
"@parity/truapi-host": patch
---

Before signing a Bulletin preimage submission, check that the product's Bulletin authorization can hold the whole payload, and ask for more first when it cannot, instead of waiting for the chain to reject the transaction.
