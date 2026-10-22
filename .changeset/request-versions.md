---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

If a product sends a request in a version the host doesn't know, the host rejects it with an `UnsupportedVersion` error instead of failing to decode it, and the call fails as `Unsupported`. Older hosts still answer `MalformedFrame`.
