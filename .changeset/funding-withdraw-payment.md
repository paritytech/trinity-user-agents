---
"@parity/truapi": patch
---

`assign_funding_withdrawal` gives an outbound funding session its withdrawal account and asks the host for the user's payment into it, as getcash pays its withdrawal key; the watch follows the payment to `Paid`, retries it under a new id, and expires it after 30 minutes untaken (Rust and native API).
