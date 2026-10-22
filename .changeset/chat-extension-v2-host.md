---
"@parity/truapi-host": minor
---

Native hosts serve Chat Extension v2 through the Swift and Kotlin packages. `ChatHostBridge` has `roomConfig`, `setRoomConfig`, `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed`, and `createRoom` and `setRoomFooter` keep their shapes. A host that does not implement them yet keeps serving products as before: the defaults accept the default configuration and the room metadata, and refuse every other request. `updateFeed` refuses a change with `NativeChatFeedRejection`. The product execution asks a product the chat queries (`chatMessagesQuery`, `chatUserAction`, `chatReactionsQuery`) with default timeouts from `ChatQueryTimeout`, and throws `ChatQueryError` (Swift) or `ChatQueryException` (Kotlin) for a query without an answer.
