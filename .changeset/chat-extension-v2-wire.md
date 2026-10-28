---
"@parity/truapi": minor
---

Add the Chat Extension v2 methods and types to `chat`. A product declares which parts of a room it serves with
`setRoomConfig` (method 7), and changes a room with `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed`.
The host asks a product questions that the product answers through `onMessagesQuery`, `onUserAction` and
`onReactionsQuery`, with one item each. `createRoom`, `postMessage` and `setRoomFooter` keep their shapes and versions,
so a product built against the earlier client calls them the same way through the client a host injects.
