---
"@parity/truapi": major
---

`payment.request` takes a caller-chosen 32-byte `id` and answers with nothing, adding `AlreadyExists` and `InvalidAmount` for a zero amount; `payment.statusSubscribe` follows that `id` and reports `ClaimedPartially`. Hosts serve both through `PaymentPlatform` (`set_payment_callbacks` and `notify_payment_status` natively); without one they answer `Unsupported`. The host receives the id hashed with the calling product, so ids never collide across products.
