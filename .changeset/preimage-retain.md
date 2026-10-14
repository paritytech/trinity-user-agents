---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`Preimage.retain` (wire id 2 of the `Preimage` trait) keeps a preimage on a cache provider for a period, paid in
advance from the cache payer of the product. The request has the key, the period, an optional size (for a chunked
file, the size of the manifest and all its chunks, so the provider keeps the chunks too) and an optional provider. The
answer has the end, the provider with its name and region, the size, the chunks and the charge. The new remote
permission `PreimageRetain` guards it. The platform side is the new method `retainPreimage` of the optional capability
`PreimageReadHost`, which shows the price and pays the provider. `truapi-host` signs a retention authorization and sends
it to the provider. A host without cache providers answers `Unsupported`.
