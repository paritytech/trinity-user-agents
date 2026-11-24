---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

The core serves the Chat Extension v2 requests of `chat`: `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and
`updateFeed`, and the configuration that each `createRoom` declares. It screens every field and checks that the
product hosts the room. A field of these requests that fails the screen fails the call with `MalformedFrame`, and a
failure of the host with `HostFailure`. A reaction in a feed update, in the reaction summaries of a message or in a
room preview is one emoji: one extended grapheme cluster of at most 64 bytes, as the chat modality RFC limits it.

A room with the default configuration is a room the host stores. In such a room a product cannot write, replace or
react as the user, quote the user, or say in the reaction summaries of a message that the user reacted. A
`postMessage` to a room that keeps its messages in the product fails with `Denied`: the product sends those messages
with `updateFeed`.

`createRoom` is one call to the host. The host keeps the configuration of the request with the room, for a new room
and for a room that exists, or refuses it with the new `UnsupportedConfig` error of `createRoom` before it creates
anything. A change of `productSource` discards the stored messages or the cached pages of the room.

Breaking for a web host that serves chat: its `chat` callbacks must now also implement `listChatRooms`,
`readChatRoomConfig`, `setChatRoomMetadata`, `setChatRoomPreview`, `deleteChatRoom` and `updateChatFeed`, and its
`createChatRoom` keeps `request.config` with the room. A typed host that leaves out a callback does not compile, and a
raw callback set without one serves no chat at all. `updateChatFeed` answers a `ChatFeedOutcome`: `Applied`, or
`UnknownMessage`, `DuplicateMessage` or `NotOwner` for an update that the host did not apply. A callback that throws
answers `HostFailure`. `readChatRoomConfig` answers the default configuration for a room that the host does not know.

Breaking for a Rust `ChatPlatform` host: `create_chat_room` keeps `request.config` with the room, or answers the new
`UnsupportedConfig` before it creates anything. `ChatPlatform` has `list_chat_rooms`, `read_chat_room_config`,
`set_chat_room_metadata`, `set_chat_room_preview`, `delete_chat_room` and `update_chat_feed`, each with a default, and
`update_chat_feed` answers a `ChatFeedOutcome`. A host whose room list subscription registers a listener implements
`list_chat_rooms`. The Rust defaults and the Swift and Kotlin bridges
have one contract: a room reads as the default configuration, a room with the default configuration and room metadata
are accepted and the metadata changes nothing, and every other request is refused.
