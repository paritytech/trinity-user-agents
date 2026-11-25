---
"@parity/truapi": major
"@parity/truapi-host": major
---

The core serves the Chat Extension v2 requests of `chat`: `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and
`updateFeed`, and the configuration that each `createRoom` declares. It screens every field and checks that the
product hosts the room. `deleteRoom` refuses only a room that the product takes part in as a bot, and passes every other
room to the host, which deletes a room that does not exist without an error. A field of these requests that fails the
screen fails the call with `MalformedFrame`, except an oversized text or custom payload in `updateFeed`, which fails it
with `MessageTooLarge`. A host that does not serve `setRoomPreview`, `deleteRoom` or `updateFeed` answers
`Unsupported`, and any other failure of the host fails the call with `HostFailure`. A room with the default
configuration is a room the host stores, and the host stores its feed updates; a room with `productSource` only shows
them.

In a room the host stores, the user writes and reacts only through the host: a feed update or a preview that names the
user as the author of a message, of the message a reply quotes, or of a reaction fails with `NotOwner`. The icon of
`setRoomMetadata` and an image badge of `setRoomPreview` are resources, as RFC 0002 names them: inline image data or
the CID of a Bulletin preimage. A URL that names where to fetch the image fails the screen. A feed update or a preview
with a contact event, a call or a payment body answers `Unsupported`: the host draws these with its own strings, and
the core does not serve them yet.

`createRoom` is one call to the host. The host keeps the configuration of the request with the room, for a new room
and for a room that exists, or refuses it before it creates anything. A change of `productSource` discards the stored
messages or the cached pages of the room.

Breaking for a v2 product: the configuration has `productSource` and `reactionDetails`. A room with `productSource`
also gets the `Seen` user action when the user sees its newest message, because RFC 0002 makes seen reports part of
the message source. `setRoomPreview` and `updateFeed` have no `RateLimited` error.

Breaking for a web host that serves chat: its `chat` callbacks must now also implement `listChatRooms`,
`readChatRoomConfig`, `setChatRoomMetadata`, `setChatRoomPreview`, `deleteChatRoom` and `updateChatFeed`, and its
`createChatRoom` keeps `request.config` with the room. `setChatRoomPreview`, `deleteChatRoom` and `updateChatFeed`
answer `"Applied"`, or `"Unsupported"` for a request the host does not serve. A typed host that leaves out a callback
does not compile, and a raw callback set without one serves no chat at all. A callback that throws fails the request
with `HostFailure`, with two exceptions: a throwing `createChatRoom` fails `createRoom` with `Unknown`, and a throwing
`listChatRooms` fails `setRoomFooter` with `Unknown`.

Breaking for a Rust `ChatPlatform` host: `create_chat_room` keeps `request.config` with the room, or refuses it before
it creates anything. `ChatPlatform` has `list_chat_rooms`, `read_chat_room_config`, `set_chat_room_metadata`,
`set_chat_room_preview`, `delete_chat_room` and `update_chat_feed`, each with a default. The last three answer a
`ChatRoomChangeOutcome`. A host whose room list subscription registers a listener implements `list_chat_rooms`. The
Rust defaults and the Swift and Kotlin bridges have one contract: a room reads as the default configuration, a room
with the default configuration and room metadata are accepted and the metadata changes nothing, and every other request
answers `Unsupported`.
