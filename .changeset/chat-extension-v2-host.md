---
"@parity/truapi-host": minor
---

Native hosts serve Chat Extension v2 through the Swift and Kotlin packages. `ChatHostBridge` has `room`,
`setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed`, and a `createRoom` that takes the configuration of
the room and keeps it with the room in one step: `createRoom(roomId:name:icon:config:)` in Swift, and a four-argument
`createRoom` in Kotlin. `room` answers how the product takes part in a room and the configuration of the room, or `nil`
(`null` in Kotlin) for a room the product never created, and the core calls it once before each change to a room. Its
default finds the room in `listRooms` and pairs it with the default configuration, so a host that keeps other
configurations overrides it. The three-argument `createRoom`, `postMessage` and `setRoomFooter` keep their shapes. A
host that does not implement the new requests yet keeps serving products as before. The four-argument `createRoom`
creates a room with the default configuration through the three-argument one, and refuses any other configuration with
`HostChatCreateRoomError.UnsupportedConfig` (`HostChatCreateRoomException` in Kotlin) before it creates anything. The
three-argument `createRoom` has a default that refuses the room, so a host that implements the four-argument one need
not implement it. `setRoomMetadata`, `setRoomPreview` and `deleteRoom` answer `ChatRoomChangeOutcome.unsupported`, and
`updateFeed` answers the `unsupported` case of its outcome (`UNSUPPORTED` in Kotlin), which the product receives as
`Unsupported`. A host that serves one of them answers `.applied` (`APPLIED`). A `HostRejection` that `createRoom` or `updateFeed` throws reaches
the product with the reason that the host wrote.
