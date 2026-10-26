---
"@parity/truapi": patch
---

`truapi-host register-name` checks the label's reservation queue on `DotnsPopController` before
signing. It stops when another account holds the first active reservation, naming that account, or,
on dotNS 1.1.0 and later, when more expired or emptied reservations stand in front of the claim than
one gateway call clears, pointing at `DotnsPopController.expireReservation(label)`. Either case would
revert in the gateway call and end in an `AccountAlias` timeout. The rule is
`truapi::host_logic::dotns_gateway::claim_blocker`.
