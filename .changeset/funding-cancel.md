---
"@parity/truapi": patch
---

`cancel_funding` cancels a funding session while nothing has arrived on its deposit account, confirmed by a fresh read; a payment after a cancel is still converted within 72 hours (Rust and native API).
