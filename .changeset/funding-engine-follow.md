---
"@parity/truapi": minor
---

The core follows payment, top-up and balance streams for native hosts. It asks the host to `follow_payment`,
`follow_top_up` or `follow_balance` and takes what the engine reports through `notify_*`. A product stream shows the
pending status after 250 ms without an answer, the last 512 final statuses are kept, an engine that stops early ends the
stream with the abandoned status, and only the main purse is offered. `payment_status`, `top_up_status` and `balance`
are gone from the callback interfaces, and `notify_balance` takes no purse.
