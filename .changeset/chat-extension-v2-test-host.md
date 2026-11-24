---
"@parity/truapi-host": minor
---

The test host serves the Chat Extension v2 callbacks: `createMockHost` keeps what a product declares for each room (the
configuration of `createRoom`, the metadata, the preview and the feed updates), readable through
`chatRoomDeclarations(roomId)` as on the Rust `MockPlatform`. It answers `"Applied"` to each preview, delete and feed
update. A `createRoom` that changes `productSource` drops the messages and feed updates of the room, and a deleted room
leaves the room list with its messages and declarations.
