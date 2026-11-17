---
"@parity/truapi-host": major
---

The core serves `payment.*` and `coinPayment.*` only from its coinage layers. A host no longer supplies a payment, top-up or balance engine, so the generated host callbacks no longer declare `PaymentPlatform`, `TopUpPlatform` or `BalancePlatform`. A host whose runtime does not run the coinage layers answers every payment call `Unsupported`, and a call made while the layers start waits for them.
