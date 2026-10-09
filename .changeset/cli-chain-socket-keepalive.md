---
"@parity/truapi": patch
---

The CLI host pings each chain WebSocket every 15 s and closes it after 45 s without any inbound frame, and a closed connection now ends its response streams, so the runtime reconnects instead of every chain call hanging on a socket the far side abandoned.
