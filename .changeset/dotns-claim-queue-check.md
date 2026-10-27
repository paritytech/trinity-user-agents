---
"@parity/truapi": patch
---

`truapi-host register-name` reads the label's reservation queue on `DotnsPopController` before
signing. It stops when another account holds the first active reservation, naming that account.
On dotNS 1.1.0 and later, when more expired or emptied reservations stand in front of the claim
than one gateway call clears, it first clears them with `DotnsPopController.expireReservation(label)`,
signed by the account, claiming PGAS into the account when its balance does not cover the fee and the
storage deposit.
Without that, the gateway call reverts and the command ends in an `AccountAlias` timeout.
