---
"@parity/truapi": patch
---

The `truapi-host` CLI opens its chain connections through `truapi-provider`'s WebSocket backend. A connection sends a ping after 20 seconds without an outgoing request, and counts as dead after 50 seconds without any incoming frame, so the core opens a new connection instead of waiting on a dead one. Messages of up to 64 MiB pass in either direction, which covers a Bulletin store transaction carrying a multi-megabyte preimage. `wss://` certificates are checked against bundled root certificates, so the binary needs no system certificate store.
