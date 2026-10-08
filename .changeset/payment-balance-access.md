---
"@parity/truapi": major
---

New `BalanceAccess` remote permission. `payment.balanceSubscribe` asks for it on the first subscription and answers `PermissionDenied` when the user refuses; a payment request refused for a short balance reaches a product without it as `Rejected`. `payment.topUp` refuses an amount of zero.
