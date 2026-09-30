---
"@parity/truapi": patch
---

`@parity/truapi/internal` exports `createMessagePortBridge` and `MessagePortBridge`, a provider for the shared container
when its host is a web page. The host transfers one `MessagePort` after the container is up, and frames sent before it
arrives wait, up to a bound, and never reach the network.

The shared container (`js/container`) uses that bridge when a web host sets `window.__truapi_message_port` to `true` or
to the host's origin before the container loads, and it takes the port only from its parent window. A native host's
`__truapi_localhost` configuration takes precedence, so the iOS and Android hosts do not change in that respect.

The container also removes the Cookie Store API (`cookieStore`) next to `document.cookie`, since both write the same
jar. That part is not limited to web hosts: the container is one bundle, so it applies to the web, iOS and Android
hosts. A product that relied on `cookieStore` there no longer finds it.
