---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native hosts serve `NftPurse.rebalancePurse`, `NftPurse.transferItem` and the new `NftPurse.observeOperationStatus`. A chain operation takes an `idempotencyKey` the product chooses and returns once it is registered; a second call under the same key fails with `OperationExists`. `observeOperationStatus` follows any operation the product registered, by its key or by the cheque id of a deposit or refund, until it is `Cleared` or `Incomplete`. Each move is a fee-less Scarcity transfer signed by its item key; an item a live move spends is no longer listed by `queryPurse`. Moving items out of a purse the product does not own asks the user through the new `NftPurseDrain` confirmation review. `deposit` and `refund` take the same shape and still answer unavailable. The browser core answers the new calls `Unsupported`.
