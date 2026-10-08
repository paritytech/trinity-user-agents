---
"@parity/truapi": major
---

Funding providers tell the host where and what the user pays with the new `FundingUpdate.Deposit` (crypto address with network, asset, amount, exactness, payment URI and expiry, or bank details with the reference), allowed until funds move. `PaymentReceived` carries `mismatch` when less or another asset arrived. `funding_progress` returns the latest deposit and mismatch. `Assigned.session` is boxed on the Rust side; the wire is unchanged.
