---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Hosts can ask for coinage consent. `RemotePermission` gains `CoinPayment`, which a product needs before its first coinage call, and `CoinPaymentPurse { purse, action }`, one action (`Query`, `Receive` or `Spend`) on one purse another product created or on the main purse. `UserConfirmationReview` gains `CoinPaymentSpend`, which a host shows before value leaves a coinage purse: the calling product, the purse, the amount in the payment asset's units, and whether the spend takes value that is still gaining privacy.

A native host that matches on `UserConfirmationReview` or `RemotePermission` handles the new cases.
