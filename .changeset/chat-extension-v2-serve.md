---
"@parity/truapi": major
"@parity/truapi-host": major
---

The core serves the Chat Extension v2 requests of `chat`: `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and
`updateFeed`, and the configuration that each `createRoom` declares. It screens every field and checks that the
product hosts the room. `deleteRoom` refuses only a room that the product takes part in as a bot, and passes every other
room to the host, which deletes a room that does not exist without an error. A field of these requests that fails the
screen fails the call with `MalformedFrame`, except an oversized text or custom payload in `updateFeed`, which fails it
with `MessageTooLarge`. A failure of the host fails the call with `HostFailure`. A room with the default configuration
is a room the host stores, and the host stores its feed updates; a room with `productSource` only shows them.

`createRoom` is one call to the host. The host keeps the configuration of the request with the room, for a new room
and for a room that exists, or refuses it before it creates anything. A change of `productSource` discards the stored
messages or the cached pages of the room.

Breaking for a v2 product: the configuration has `productSource` and `reactionDetails`. A room with `productSource`
also gets the `Seen` user action when the user sees its newest message, because RFC 0002 makes seen reports part of
the message source. `setRoomPreview` and `updateFeed` have no `RateLimited` error.

Breaking for a web host that serves chat: its `chat` callbacks must now also implement `listChatRooms`,
`readChatRoomConfig`, `setChatRoomMetadata`, `setChatRoomPreview`, `deleteChatRoom` and `updateChatFeed`, and its
`createChatRoom` keeps `request.config` with the room. A typed host that leaves out a callback does not compile, and a
raw callback set without one serves no chat at all. A callback that throws fails the request with `HostFailure`, with
two exceptions: a throwing `createChatRoom` fails `createRoom` with `Unknown`, and a throwing `listChatRooms` fails
`setRoomFooter` with `Unknown`.

Breaking for a Rust `ChatPlatform` host: `create_chat_room` keeps `request.config` with the room, or refuses it before
it creates anything. `ChatPlatform` has `list_chat_rooms`, `read_chat_room_config`, `set_chat_room_metadata`,
`set_chat_room_preview`, `delete_chat_room` and `update_chat_feed`, each with a default. A host whose room list
subscription registers a listener implements `list_chat_rooms`. The Rust defaults and the Swift and Kotlin bridges
have one contract: a room reads as the default configuration, a room with the default configuration and room metadata
are accepted and the metadata changes nothing, and every other request is refused.
