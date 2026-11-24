---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

A host answers a feed update with a `ChatFeedOutcome`: `Applied`, or `UnknownMessage`, `DuplicateMessage` or `NotOwner`
for an update that it did not apply, which the product receives as the error of the same name. The web callback
`updateChatFeed`, `updateFeed` of the Swift and Kotlin bridges and `ChatPlatform::update_chat_feed` return it, and throw
only for a failure of the host, which the product receives as `HostFailure`.

In a room the host stores, the core answers `NotOwner` to a feed update or a room preview that writes, replaces or
reacts as the user, quotes the user, or counts the user in the reaction summaries of a message. The core reads the
configuration of the room before each preview too.

The core bounds the v2 chat fields that a host draws. A reaction in a feed update, in the reaction summaries of a
message or in a room preview is one emoji: one extended grapheme cluster of at most 64 bytes. A message has at most 64
reaction summaries, a room description or placeholder is at most 1 KiB, and a HOP file names a `wss://` node and a size
from 1 byte to 128 MiB. A field over a bound fails the call with `MalformedFrame` before the host sees it.
