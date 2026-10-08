---
"@parity/truapi": major
---

`payment.topUp` takes a caller-chosen 32-byte `id`, answers `InvalidSource`, `AlreadyExists`, `SourceBusy` or `Unknown`, and is followed with the new `payment.topUpStatusSubscribe`. The core requires a session, validates the source keys, and hands the top-up to the host's `TopUpPlatform`, installed with `set_top_up_platform`; without one, both methods answer `Unsupported`. The host receives the id hashed with the calling product, so ids never collide across products.
