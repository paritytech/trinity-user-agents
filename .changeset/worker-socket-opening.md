---
"@parity/truapi": patch
---

The native WebView container keeps one WebSocket opening while its host is suspended. Calls waiting for it can time out,
and the worker uses that connection when the host resumes.
