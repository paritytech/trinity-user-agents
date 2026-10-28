---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native hosts serve `NftPurse.revokeCheque` and `NftPurse.refund`. `revokeCheque` moves the items at the keys of a cheque the product created back into fresh keys of its purse, and can be repeated to retry the keys whose move failed; another product's cheque answers `ChequeNotFound`, and `NftPurseRevokeChequeError.Denied` is gone. `refund` returns the items a product deposited from a cheque to the cheque's keys once the deposit has settled, and answers which items move and which stay, with the new `NftPurseRefundError.DepositInProgress` and `ItemUnavailable`. Both are followed by `observeOperationStatus` under `blake2_256("nft-purse/revoke" ‖ chequeId)` and `blake2_256("nft-purse/refund" ‖ chequeId)`. Refunding out of a purse the product does not own asks the user through `NftPurseDrain` with the new `Refund` destination. A transfer now moves whatever its item key holds on its first attempt. The browser core answers both calls `Unsupported`.
