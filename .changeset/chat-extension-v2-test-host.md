---
"@parity/truapi-host": minor
---

The test host serves the Chat Extension v2 callbacks: `createMockHost` keeps what a product declares for each room (the
configuration of `createRoom`, the metadata, the preview and the feed updates), readable through
`chatRoomDeclarations(roomId)` as on the Rust `MockPlatform`. It answers each feed update `Applied`, and a deleted room
leaves the room list.
