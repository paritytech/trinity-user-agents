---
"@parity/truapi": major
---

Funding providers tell the host where and what the user pays with the new `FundingUpdate.Deposit` (crypto address with network, asset, amount, exactness, payment URI and expiry, or bank details with the reference), allowed until funds move. The core takes only instructions that name something to pay, have not expired, carry a payment URI rather than a web, script or file one, keep their texts to one short plain line, and pay by the rail and asset of the quote the user chose; anything else, or more than eight per session, is refused with the new `HostFundingReportError.InvalidDetail`. `PaymentReceived` carries `mismatch` when less or another asset arrived. A session's progress carries the latest deposit and mismatch. `Assigned.session` is boxed on the Rust side; the wire is unchanged.
