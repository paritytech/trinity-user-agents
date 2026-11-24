---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

A host answers a feed update with a `ChatFeedOutcome`, not a `ChatRoomChangeOutcome`: `Applied`; `UnknownMessage`,
`DuplicateMessage` or `NotOwner` for an update that it did not apply, which the product receives as the error of the
same name; or `Unsupported` for a host that does not serve feed updates, which the product receives as `Unsupported`.
The web callback `updateChatFeed`, `updateFeed` of the Swift and Kotlin bridges and `ChatPlatform::update_chat_feed`
return it, and throw only for a failure of the host, which the product receives as `HostFailure`. Their defaults answer
`Unsupported` (`ChatFeedOutcome.unsupported` in Swift, `ChatFeedOutcome.UNSUPPORTED` in Kotlin). A preview and a delete
answer a `ChatRoomChangeOutcome`.

The core bounds the v2 chat fields that a host draws. A reaction in a feed update, in the reaction summaries of a
message or in a room preview is one emoji: one extended grapheme cluster of at most 64 bytes. A message has at most 64
reaction summaries, a room description or placeholder is at most 1 KiB, and a HOP file names a `wss://` node of at
most 256 characters and a size from 1 byte to 128 MiB. A field over a bound fails the call with `MalformedFrame` before
the host sees it.
