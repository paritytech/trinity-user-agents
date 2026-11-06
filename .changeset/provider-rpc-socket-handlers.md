---
"@parity/truapi-provider": patch
---

The browser RPC backend's WebSocket no longer throws "closure invoked recursively or after being dropped" into the page. A connection that is closed and released, a connect whose handshake fails, and a connect that is abandoned before its handshake ends each take the socket's event handlers off before the callbacks they call are freed, and close the socket.
