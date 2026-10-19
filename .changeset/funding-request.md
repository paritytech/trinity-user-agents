---
"@parity/truapi": minor
---

Add `Funding` (trait 22): `request({ id, direction, amount? })` opens a session under a caller-chosen id and shows the host's funding overlay, and `statusSubscribe({ id })` lets the product that opened it watch it to completion, also after a reload. Rust embedders install `FundingPlatform` with `set_funding_platform` and open sessions of their own with `open_funding`.
