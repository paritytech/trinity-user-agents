---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`coinPayment` is served by the host's coinage layer on a signing host that turns it on, instead of answering every call as unsupported. Three methods are added: `chequeStatus` follows what became of one of the caller's cheques, `cancelCheque` takes back what a payee has not claimed, and `listPurses` lists the purses the caller may see.

- A product needs the new `CoinPayment` permission before its first coinage call, and the `CoinPaymentPurse` permission for each action on a purse another product created or on the main purse.
- The user confirms every spend through the new `CoinPaymentSpend` review.
- A host that runs no coinage layer, including a paired host and the browser, still answers every `coinPayment` call as unsupported.
