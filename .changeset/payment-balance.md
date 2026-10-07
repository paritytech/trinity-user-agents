---
"@parity/truapi": minor
---

`payment.balanceSubscribe` streams the user's spendable balance from the host's new `BalancePlatform` (`set_balance_callbacks` and `notify_balance` natively): the host answers the current balance or `PermissionDenied`, then pushes each change. The core requires a session; without a balance view the call is `Unsupported`.
