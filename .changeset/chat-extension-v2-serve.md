---
"@parity/truapi-host": minor
---

The core serves the Chat Extension v2 requests of `chat`: `setRoomConfig`, `setRoomMetadata`, `setRoomPreview`,
`deleteRoom` and `updateFeed`. It screens every field, checks that the product hosts the room, and paces `updateFeed`
and `setRoomPreview` for each room. A room the product declares no configuration for is a room the host stores, and in
such a room a product cannot write a message as the user. A host that serves chat implements the new `ChatPlatform`
callbacks, which default to a rejection, so a host without them keeps serving products as before.
