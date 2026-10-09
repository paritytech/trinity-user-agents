---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`Preimage.read` (wire id 3 of the `Preimage` trait) reads a preimage once, through a route that the product chooses:
`Auto`, `Bulletin`, `Cache`, or one `CacheProvider`. It answers the value with a read report: the source that served it
(the host's own cache, a cache provider with its origin, rank and home flag, or Bulletin) and every source that the host
asked, with the outcome and the time of each. `skipHostCaches` makes the host read past its own caches, so a product can
measure the network. The platform side is the new optional capability `PreimageReadHost` (`preimageRead.readPreimage`
for web hosts). A host without it answers `Unsupported`, so existing hosts keep working. `truapi-host` serves it through
its cache nodes and its Bulletin node, and its provider set lines accept `name=` and `region=`.
