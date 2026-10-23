---
"@parity/truapi": minor
---

`renderer.actionSubscribe()` and `chat.actionSubscribe()` survive a lost host connection. The host holds both kinds of action while no stream is open, so the client keeps the stream when the connection resets and starts it again on the next connection the host connection verifies. Presses and messages from while the connection was down arrive once it is back, and the product's observer never sees the break. A reconnect the host refuses while it rebinds its listener leaves the stream waiting for the one after, and the stream never dials on its own. It still ends when the host connection is disposed.

A method opts in with `#[wire(id = N, reopen_on_reset)]`, which the generated client passes to the transport as the `reopenOnReset` option on `subscribeRaw`. Providers report a verified connection through the optional `subscribeReady` hook on `WireProvider`; a provider without it closes these streams on reset like any other.
