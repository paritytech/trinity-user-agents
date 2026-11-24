---
"@parity/truapi": minor
---

`cancel_funding` now cancels a funding session in any state, so a host needs no other call to stop one. While the session's overlay is up, the core stops starting it on its best quote, drops its `present_funding` call (the host takes the overlay down) and drops the session, answering `Cancelled`; when the user already chose a provider there, the session runs as started and the provider is asked to stop (`Requested`). Started sessions are cancelled as before, and an ended or unknown session answers `Refused`.

`CancelOutcome::Ended` is renamed `Cancelled` (`.cancelled` in Swift, `CANCELLED` in Kotlin), and the `result` tag on `truapi.funding.cancel_requested` reads `cancelled` instead of `ended`. Hosts matching on the old case have to rename it.
