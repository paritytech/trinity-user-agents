---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`Preimage.cacheAccount` (wire id 5 of the `Preimage` trait) gives the cache account of the payer with which the host
pays cache providers for the product: what is left of its free allowance today and the part of it for retention, its
paid credits, the daily allowance, and the prices of reads and of retention. The host asks a cache provider, the one
of the request or the first of its order. The platform side is the new method `cacheAccount` of the optional
capability `PreimageReadHost`. `PreimageReadSource::CacheProvider` of a read report gets `price`, the price of the read
that the provider gave. A host without cache providers answers `Unsupported`.
