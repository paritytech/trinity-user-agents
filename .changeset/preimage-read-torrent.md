---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`Preimage.readTorrent` (wire id 4 of the `Preimage` trait) reads one file of a BitTorrent v1 torrent once, through cache
providers: a magnet link, an optional file index, and a route (`Auto`, `Cache` or one `CacheProvider`; `Bulletin` is
refused). It answers the file, its preimage key (blake2b-256, set by the core), the file name and index, and a read
report as for `Preimage.read`. The core keeps the file in its read-after-write cache. The platform side is the new
method `readTorrent` of the optional capability `PreimageReadHost`: a web host gives `preimageRead.readTorrent` next to
`readPreimage`, and checks the file against the piece hashes of the torrent before it pays a provider. A host without it
answers `Unsupported`. `truapi-host` answers that it does not read torrents.
