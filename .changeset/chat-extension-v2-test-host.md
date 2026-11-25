---
"@parity/truapi-host": minor
---

The test host serves the Chat Extension v2 callbacks: `createMockHost` keeps what a product declares for each room (the
configuration of `createRoom`, the metadata, the preview and the feed updates), readable through
`chatRoomDeclarations(roomId)` as on the Rust `MockPlatform`. Each feed update is a `ChatFeedRecord`: the update, and
whether the host had to store it. The test host answers `"Applied"` to each metadata change, preview, delete and feed
update.

A `createRoom` that changes `productSource` drops the messages, the feed updates and the preview of the room. A delete
removes the room from the room list, with its messages and its declarations.
