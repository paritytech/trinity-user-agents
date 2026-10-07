---
"@parity/truapi": major
---

`payment.request` takes a caller-chosen 32-byte `id` and answers with nothing, adding `AlreadyExists`; `payment.statusSubscribe` follows that `id` and reports `PartiallyClaimed`. Hosts serve both through `PaymentPlatform` (`set_payment_callbacks` and `notify_payment_status` natively); without one they answer `Unsupported`.
