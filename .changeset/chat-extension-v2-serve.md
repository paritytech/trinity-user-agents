---
"@parity/truapi": major
"@parity/truapi-host": major
---

The core serves the Chat Extension v2 requests of `chat`: `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and
`updateFeed`, and the configuration that each `createRoom` declares. It screens every field and checks that the product
hosts the room. A field of these requests that fails the screen fails the call with `MalformedFrame`, except an
oversized text or custom payload in `updateFeed`, which fails it with `MessageTooLarge`.

`deleteRoom` refuses only a room that the product takes part in as a bot, and passes every other room to the host,
which deletes a room that does not exist without an error. A host that does not serve `setRoomMetadata`,
`setRoomPreview`, `deleteRoom` or `updateFeed` answers `Unsupported`, and any other failure of the host fails the call
with `HostFailure`. A room with the default configuration is a room the host stores, and the host stores its feed
updates; a room with `productSource` only shows them.

In a room the host stores, the user writes and reacts only through the host: a feed update or a preview that names the
user as the author of a message, of the message a reply quotes, or of a reaction fails with `NotOwner`.

The icon of a version 2 `createRoom`, the icon of `setRoomMetadata` and an image badge of `setRoomPreview` are
resources, as RFC 0002 names them: an inline image as a `data:` URL with a Base64 payload, or the CID of a Bulletin
preimage. Any other text fails the screen, a URL included, and fails `createRoom` with `Unknown`. A version 1
`createRoom` keeps its icon rules.

`createRoom` is one call to the host. The host keeps the configuration of the request with the room, for a new room
and for a room that exists, or refuses it before it creates anything. A change of `productSource` discards the stored
messages or the cached pages of the room, and its preview.

Breaking for a v2 product: the configuration has `productSource` and `reactionDetails`. A room with `productSource`
also gets the `Seen` user action when the user sees its newest message, because RFC 0002 makes seen reports part of
the message source. `setRoomPreview` and `updateFeed` have no `RateLimited` error. The version 2 `createRoom` request
of 0.33.0 and of this release do not decode each other: a product built against 0.33.0 gets `MalformedFrame` from a
host with this release, and a product built against this release gets it from a 0.33.0 host. A version 1 `createRoom`
is not affected.

Breaking for a web host that serves chat: its `chat` callbacks must now also implement `chatRoom`,
`setChatRoomMetadata`, `setChatRoomPreview`, `deleteChatRoom` and `updateChatFeed`, and its `createChatRoom` keeps
`request.config` with the room. `chatRoom` answers how the product takes part in a room and the configuration of the
room, or `undefined` for a room the product never created, and the core calls it once before each change to a room.
`setChatRoomMetadata`, `setChatRoomPreview`, `deleteChatRoom` and `updateChatFeed` answer `"Applied"`, or
`"Unsupported"` for a request the host does not serve. A typed host that leaves out a callback does not compile, and a
raw callback set without one serves no chat at all. A callback that throws fails the request with `HostFailure`, with
two exceptions: a throwing `createChatRoom` fails `createRoom` with `Unknown`, and a throwing `chatRoom` fails
`setRoomFooter` with `Unknown`.

Breaking for a Rust `ChatPlatform` host: `create_chat_room` keeps `request.config` with the room, or refuses it before
it creates anything. It has no default, so a host that keeps only the default configuration refuses any other itself.
`ChatPlatform` has a required `chat_room`, which answers a `ChatRoomState`, how the product takes part in the room and
the configuration of the room, or `None` for a room the product never created; the core calls it once before each
change to a room, and a host that cannot read its rooms returns an error. `ChatPlatform` also has
`set_chat_room_metadata`, `set_chat_room_preview`, `delete_chat_room` and `update_chat_feed`. Each answers a
`ChatRoomChangeOutcome`, and each has a default that answers `Unsupported`.

The chat host of the `truapi-host` CLI keeps only the default configuration and refuses any other, and it answers
`Unsupported` to `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed`.
