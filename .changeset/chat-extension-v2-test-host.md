---
"@parity/truapi-host": major
---

Hosts ask a Chat Extension v2 product their six chat queries through the provider: `chatMessagesQuery`,
`chatUserAction`, `chatReactionsQuery`, `chatParticipantsQuery`, `chatMessageActionsQuery` and
`chatParticipantActionsQuery` each take the request and `{ timeoutMs, signal }` and resolve with the item or a typed
refusal (the `CallError` of the query, `Timeout` or `NoAnswer`), with the time limits of the RFC as defaults.
`publishChatEvent` publishes a Chat event of the current version. The page host carries both to the Web Worker runtime.
`publishChatAction` now takes an item of the first version (`V01HostChatActionSubscribeItem`), which is what the core
decodes.

The test host serves the ten new chat callbacks: `createMockHost` keeps each room's declarations, its feed (stored for a
host-store room, on the screen only for a product-source room) and its chat list row, readable through `rooms()`,
`room(roomId)`, `feed(roomId)`, `preview(roomId)` and `events()`, and `host.chatUser` plays the user against the product
through the queries and events. `createMockClient` returns that runtime as `chat`, takes `executionKind` from
`runtimeConfig`, can reuse a `host` to play a product restart, and can keep preimage submissions and allowances in the
core (`submitPreimagesLocally`, `grantAllowances`).
