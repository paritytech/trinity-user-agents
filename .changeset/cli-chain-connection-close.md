---
"@parity/truapi": patch
---

The CLI host ends a chain connection's response stream when its WebSocket closes, so the runtime opens a fresh connection for the next request rather than queueing it on the closed socket.
