---
"@parity/truapi-host": minor
---

On a signing host that turns coinage on, `payment.balanceSubscribe`, `payment.topUp`, `payment.request` and their status streams are served by the coinage layers instead of the host's own payment, top-up and balance engines, and funding sessions follow them by id in the same way.

- The balance is the purse's spendable value in the payment asset's units. An amount moves rounded down to a whole cent, and less than a cent is refused as `InvalidAmount`.
- Every payment is confirmed by the user through the `CoinPaymentSpend` review. A purse other than the main purse needs the `CoinPayment` and `CoinPaymentPurse` permissions.
- A top-up from a product account or a private key waits up to an hour for the source's funds; only one top-up per source runs at a time (`SourceBusy`). A top-up from coins imports them.
- Without coinage, the host's own engines keep serving these calls.
