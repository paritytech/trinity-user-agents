---
"@parity/truapi": minor
"@parity/truapi-host": patch
---

Host-initiated methods returning `Result` generate asynchronous Rust callers and TypeScript handlers returning typed
`neverthrow` results over one-item subscriptions. Calls support cooperative cancellation, startup buffering, and
connection cleanup through the existing subscription transport.
