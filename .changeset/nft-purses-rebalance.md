---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native hosts serve `NftPurse.rebalancePurse`, `NftPurse.transferItem` and the new `NftPurse.observeOperationStatus`. A chain operation takes an `idempotencyKey` the product chooses and returns once it is registered; a second call under the same key fails with `OperationExists`. `observeOperationStatus` follows any operation the product registered, by its key, until it is `Cleared` or `Incomplete`; deposits and refunds will be followed by their cheque id once they are served. Each transfer is a fee-less Scarcity transfer signed by its item key; an item a live transfer spends is no longer listed by `queryPurse`. Transferring items out of a purse the product does not own asks the user through the new `NftPurseDrain` confirmation review. `deposit` and `refund` take the same shape and still answer unavailable. The browser core answers the new calls `Unsupported`.
