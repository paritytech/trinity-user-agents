---
"@parity/truapi": minor
---

Add `Funding` (trait 21): `request(direction, amount?)` opens a session and shows the host's funding overlay, and `statusSubscribe` lets the product that opened it watch it to completion. Rust embedders install `FundingPlatform` with `set_funding_platform` and open sessions of their own with `open_funding`.
