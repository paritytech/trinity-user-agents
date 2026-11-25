---
"@parity/truapi-host": minor
---

Native hosts serve Chat Extension v2 through the Swift and Kotlin packages. `ChatHostBridge` has `room`,
`setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed`, and a `createRoom` that takes the configuration of
the room and keeps it with the room in one step: `createRoom(roomId:name:icon:config:)` in Swift, and a four-argument
`createRoom` in Kotlin.

`room` answers how the product takes part in a room and the configuration of the room, or `nil` (`null` in Kotlin) for
a room the product never created, and the core calls it once before each change to a room. Its default finds the room
in `listRooms` and pairs it with the default configuration, so a host that keeps other configurations overrides it.

A host that does not implement the new requests yet keeps serving products as before. The three-argument `createRoom`,
`postMessage` and `setRoomFooter` keep their shapes. The four-argument `createRoom` creates a room with the default
configuration through the three-argument one, and refuses any other configuration with
`HostChatCreateRoomError.Unknown` (`HostChatCreateRoomException` in Kotlin) before it creates anything. The
three-argument `createRoom` has a default that refuses the room, so a host that implements the four-argument one need
not implement it.

`setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed` answer a `ChatRoomChangeOutcome`: `.unsupported` by
default (`UNSUPPORTED` in Kotlin), which the product receives as `Unsupported`, and `.applied` (`APPLIED`) from a host
that serves the request. A `HostRejection` that a chat callback other than `listRooms` throws reaches the product with
the reason that the host wrote.
