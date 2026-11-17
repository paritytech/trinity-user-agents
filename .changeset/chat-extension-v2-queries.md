---
"@parity/truapi-host": minor
---

The host asks a product the Chat Extension v2 queries of a room through the product execution: `chatMessagesQuery`,
`chatUserAction` and `chatReactionsQuery`, in Swift and Kotlin. Each query waits for one answer within a default timeout
from `ChatQueryTimeout`, and throws `ChatQueryError` (Swift) or `ChatQueryException` (Kotlin) for a query without one.
The core asks the product only for a room whose configuration declares the handler, and answers `NotAvailable` itself
for any other room without starting the worker. It screens the request before the product sees it and the answer before
the host sees it.
