---
"@parity/truapi": minor
---

`@parity/truapi/internal` exports `createWebSocketProviderFactory`, so a host container can wrap the provider it passes to `createHostConnection`. The factory captures the native WebSocket APIs when it is called, so call it before product code runs.
