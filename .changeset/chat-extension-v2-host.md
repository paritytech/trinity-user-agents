---
"@parity/truapi-host": minor
---

Native hosts serve Chat Extension v2 through the Swift and Kotlin packages. `ChatHostBridge` has `roomConfig`,
`setRoomConfig`, `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed`, and `createRoom` and
`setRoomFooter` keep their shapes. A host that does not implement them yet keeps serving products as before: the
defaults accept the default configuration and the room metadata, and refuse every other request. `updateFeed` refuses a
change with `NativeChatFeedRejection`.
