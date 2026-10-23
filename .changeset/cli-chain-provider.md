---
"@parity/truapi": patch
---

The `truapi-host` CLI opens its chain connections through `truapi-provider`'s WebSocket backend. A connection sends a ping after 20 seconds without an outgoing request, and counts as dead after 50 seconds without any incoming frame, so the core opens a new connection instead of waiting on a dead one. Messages of up to 64 MiB pass in either direction, so the full body of a busy Bulletin block can be fetched. `wss://` certificates are checked against bundled webpki root certificates, not the operating system's trust store.
