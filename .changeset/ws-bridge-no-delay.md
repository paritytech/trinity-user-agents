---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

The WebSocket bridge sends each frame at once instead of letting the socket hold small frames back (Nagle's algorithm),
so a small frame such as a gesture move no longer waits for the peer's delayed acknowledgement of the previous one
(at least 40 ms on Android).
