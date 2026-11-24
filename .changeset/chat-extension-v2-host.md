---
"@parity/truapi-host": minor
---

Native hosts serve Chat Extension v2 through the Swift and Kotlin packages. `ChatHostBridge` has `roomConfig`,
`setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed`, and a `createRoom` that takes the configuration of
the room and keeps it with the room in one step: `createRoom(roomId:name:icon:config:)` in Swift, and a four-argument
`createRoom` in Kotlin. The three-argument `createRoom`, `postMessage` and `setRoomFooter` keep their shapes. A host
that does not implement the new requests yet keeps serving products as before. The four-argument `createRoom` creates
a room with the default configuration through the three-argument one, and refuses any other configuration with
`NativeChatRoomRejection.UnsupportedConfig` before it creates anything. The other defaults accept the room metadata and
refuse every other request. `updateFeed` refuses a change with `NativeChatFeedRejection`. A `HostRejection` that
`createRoom` or `updateFeed` throws reaches the product with the reason that the host wrote.
