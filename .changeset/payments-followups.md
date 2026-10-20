---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

`payment.balanceSubscribe` checks the product's `BalanceAccess` again before passing on each balance, so revoking the
grant ends the stream with `PermissionDenied`. The new `AlreadyExists` and `InvalidAmount` errors follow the existing
variants, so `HostPaymentError` keeps the indexes it had before. The test host says why it cannot serve payments: the
payment platforms are served only by native and Rust hosts.
