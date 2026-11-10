---
"@parity/truapi-host": minor
---

Native hosts observe public profile metadata as a finite stream. Required resolution options let callers force a dotNS lookup and suppress stale stored values independently. Stored metadata arrives first unless suppressed; a changed chain result follows when the record needs refreshing or the caller forces a lookup. An unchanged result renews freshness and is emitted only when no stored value was emitted. The core retains up to 1,000 records by last use, independently of its one-hour freshness policy. Hosts cache verified image bytes separately by CID through the Bulletin image loader. Native observations report completion and typed errors and support cancellation; the Rust WASM stream resolves one profile without a record cache.
