---
"@parity/truapi": patch
---

The legacy `__HOST_API_PORT__` bridge now survives a connection reset. It is bound to the host connection rather than to one socket, resolving the live connection per frame, so it keeps working once a replacement is verified and is closed only when the connection itself is disposed.

Before, every retire closed the bridge and nothing rebuilt it, because `legacyPort` stays memoised so its getter returns the same port forever. A product that adopted the port at startup cannot re-read `__HOST_API_PORT__`, so it was left holding a port with no peer, and a port whose peer closed raises no event. The product saw no error and no close, its calls went nowhere, and nothing short of a reload recovered. The injected client reconnected normally throughout, so the host looked healthy while every product call on the legacy path hung.

This reaches products whose bundled `@parity/truapi` predates the injected client and therefore still take the port path.
