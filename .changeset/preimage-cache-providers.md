---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`Preimage.cacheProviders` (wire id 6 of the `Preimage` trait) gives the cache providers of the host, in its order for a
target (a key, a CID or a DotNS name): the home providers first, then by measured latency. Each has its id, name,
region, operator, source, latency and whether a read failed a short time ago. A product shows them so that a person can
pick the provider of a retention. The platform side is the new method `cacheProviders` of `PreimageReadHost`.
