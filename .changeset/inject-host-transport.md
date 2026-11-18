---
"@parity/truapi": major
---

A product inside a native host encodes its own frames. The host's container injects `window.__HOST_API_TRANSPORT__`, a frame channel over the host's connection, and the SDK builds its own client over it. A product therefore speaks the protocol version it was built against, whatever version of `@parity/truapi` the host was built with.

- `@parity/truapi/sandbox` exports the `HostTransport` interface a host implements. The SDK only knows the interface.
- The SDK does not read `window.__HOST_API_CLIENT__`, and hosts do not publish it. SDKs from 0.19.0 that looked for it use `window.__HOST_API_PORT__` instead, which also carries frames they encode themselves.
- `window.__HOST_API_PORT__` survives a replaced host connection. A frame sent with no connection waits for the next one. A call or subscription the lost connection carried ends with a `HostFailure` error, and a stream the host started is stopped.
- `@parity/truapi/internal` exports `createWebSocketProviderFactory`. `createHostConnection` and `HostConnection` are not exported: the connection is the host's and lives in the host's container.
- A call the host lost fails with the `ConnectionResetError` of the product's own SDK, and the client stays usable across reconnects.
