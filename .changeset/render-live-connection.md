---
"@parity/truapi": patch
---

A native product execution renders on the newest product connection that is still open. `render` answers
`NotConnected`, which hosts already wait out, when every connection of the execution has ended, and `Closed` only
for a closed execution. A connection that ends closes only its own Chat and Renderer action and failure
subscriptions, so taps keep reaching a connection of the same execution that is still open.
