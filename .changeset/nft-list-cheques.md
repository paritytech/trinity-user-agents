---
"@parity/truapi": minor
---

`nftPurse.listCheques` returns every cheque the calling product created on this host, oldest first, exactly as
`createCheque` returned it, with the purse its items left, its destination and its creation time. A product that lost a
cheque can deliver it again or revoke it.
