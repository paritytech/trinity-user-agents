---
"@parity/truapi-host": minor
---

The core serves the Chat Extension v2 requests of `chat`: `setRoomConfig`, `setRoomMetadata`, `setRoomPreview`,
`deleteRoom` and `updateFeed`. It screens every field, checks that the product hosts the room, and paces `updateFeed`
and `setRoomPreview` for each room. A room the product declares no configuration for is a room the host stores, and in
such a room a product cannot write, replace or react as the user.

Breaking for a web host that serves chat: its `chat` callbacks must now also implement `listChatRooms`,
`readChatRoomConfig`, `setChatRoomConfig`, `setChatRoomMetadata`, `setChatRoomPreview`, `deleteChatRoom` and
`updateChatFeed`. A host that leaves out any chat callback serves no chat at all. A web host's `updateChatFeed` can
refuse an update only with `Unknown`: the web bridge turns each error that a web callback throws into `Unknown`. A Rust `ChatPlatform` host and the
Swift and Kotlin bridges get defaults with one contract: a room reads as the default configuration, a declaration of
the default configuration or of room metadata is accepted and changes nothing, and every other request is refused.
