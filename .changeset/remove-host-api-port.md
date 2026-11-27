---
"@parity/truapi": major
---

Native hosts and the `truapi-host` CLI publish `window.__HOST_API_TRANSPORT__` and no `window.__HOST_API_PORT__`, so
products need `@parity/truapi` 0.32 or later.

Outside an iframe, the SDK connects only over an injected transport. A host that injects only `__HOST_API_PORT__` does
not connect products built with this release: `isCorrectEnvironment` is false there and `getClientSync` returns `null`.
The SDK's `Window` interface declares no `__HOST_WEBVIEW_MARK__`.
