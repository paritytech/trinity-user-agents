---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

When a product signs with another product's account, the user is asked once for each kind of signature and each owning
product, rather than for every signature. The approval lasts until the calling product is closed. This applies to
payload, watermarked raw, transaction and statement proof signatures. If the user declines, the next signature asks
again. Transactions that name contacts and unwatermarked raw bytes still ask every time. Only one of these prompts is
open at a time, and a signature the user already approved does not wait for it.
