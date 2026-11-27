---
"@parity/truapi": patch
---

When reconnecting to a suspended host, the native WebView container gives one WebSocket up to two minutes to open. Calls
can time out sooner. If the upgrade stalls, the container closes the socket so the worker can reconnect.
