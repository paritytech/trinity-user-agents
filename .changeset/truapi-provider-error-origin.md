---
"@parity/truapi-provider": patch
---

Handshake and WebSocket creation errors name a node by its scheme, host and port, without the path or query that can carry an API key.
