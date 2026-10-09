---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

A product that signs with another product's account asks the user once for each kind of signature and each owning
product, until the product closes, instead of on every signature. This applies to payload, watermarked raw, transaction
and statement proof signatures. If the user declines, the next signature asks again. Transactions that name contacts and
unwatermarked raw bytes still ask every time. Only one of these prompts is open at a time, and a signature the user
already approved does not wait for it.
