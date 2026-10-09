---
"@parity/truapi-host": patch
---

Keep SSO account requests bound to their authenticated wallet session and withdraw submitted requests when explicit cancellation drops the waiting operation. Order paired-session activation after pending grant cleanup. A paired host no longer prompts before forwarding a wallet request; the phone reviews it, as with the Android app. A product reset no longer interrupts a paired session that is being activated.
