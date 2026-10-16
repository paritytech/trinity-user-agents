---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

The core derives the cache payer of each person and product with a BLAKE2b key that a product cannot ask for through
`entropy.derive`, and gives its seed to the platform with the new `PreimageReadHost.setCachePayer(seed)` before each
cache call. An empty seed means no signed-in person. The seed never reaches a product.
