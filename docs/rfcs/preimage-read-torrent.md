---
title: "Torrent reads through cache providers via Preimage.readTorrent"
owner: "Leonardo Custodio"
status: draft
---

# RFC: Torrent reads through cache providers via `Preimage.readTorrent`

|                 |                                                                                                                                                                  |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-10-13                                                                                                                                                       |
| **Description** | Add a `readTorrent` method to the `Preimage` trait. It reads one file of a BitTorrent torrent through cache providers, and gives the file with its preimage key. |
| **Authors**     | Leonardo Custodio                                                                                                                                                |

## Summary

Cache providers can fetch content from BitTorrent (`cache/docs/bittorrent-plan.md`). A product cannot use that today:
TrUAPI names content only by a preimage key, the blake2b-256 of the value, and a product with a magnet link does not
know the key of the file. This RFC adds `Preimage.readTorrent`:

- The request has a magnet link, an optional file index, and a route (`PreimageReadRoute` of `Preimage.read`).
- The response has the file, its preimage key, the file name and index, and a `PreimageReadReport`.
- After the read, the providers keep the file under its preimage key too, so the product reads it again with
  `Preimage.read` and the key, as any preimage.

An old host answers `Unsupported`.

## Wire

```rust
#[wire(id = 4)]
async fn read_torrent(&self, cx: &CallContext, request: RemotePreimageReadTorrentRequest)
    -> Result<RemotePreimageReadTorrentResponse, CallError<RemotePreimageReadError>>;

pub struct RemotePreimageReadTorrentRequest {
    pub magnet: String,          // magnet:?xt=urn:btih:… (v1), with so= and x.pe if the product has them
    pub file: Option<u32>,       // None: so= of the magnet link, else file 0
    pub route: PreimageReadRoute, // Auto, Cache or CacheProvider; Bulletin is refused
}

pub struct RemotePreimageReadTorrentResponse {
    pub value: Option<Vec<u8>>,  // the file, checked against the piece hashes
    pub key: Option<[u8; 32]>,   // blake2b-256 of value, set by the core
    pub name: Option<String>,    // the path of the file in the torrent
    pub file: u32,
    pub report: PreimageReadReport,
}
```

Wire id 2 stays free for `Preimage.retain`. Errors use `PreimageReadError`.

## Trust

The core cannot check a torrent file against a key that the product gives, because the product has no key. The check is
against the torrent:

1. The host asks a provider for the file by its torrent id `bt:<infohash>:<file>`. The provider sends the span: the
   pieces that hold the file.
2. The host gets the info dict (`GET /torrent/<infohash>` of the provider), and checks that its SHA-1 is the infohash.
3. The host checks the SHA-1 of each piece of the span, takes the file from the span, and only then pays the provider.
4. The core sets the key from the bytes, and keeps the file in its read-after-write cache.

A provider that sends bad pieces or a wrong info dict gets no receipt, and its attempt is `BadBytes`. BitTorrent v1 uses
SHA-1, so a torrent is as honest as its author: its author can make two different files with one infohash.

## Platform

`PreimageReadHost` gets a second method, `read_torrent(request)`. A web host gives `preimageRead.readTorrent` next to
`readPreimage`; the capability counts only when both are there. `truapi-host` answers that it does not read torrents.

## Open questions

- Large files: the host holds the whole file in memory, as for `Preimage.read`.
- A host could list the files of a torrent before the read. Today a multi-file torrent without a file index fails, and
  the reason lists the files.
