---
"@parity/truapi-host": major
---

The test host serves the Chat Extension v2 callbacks: `createMockHost` keeps what a product declares for each room
(configuration, metadata, preview, feed updates, features, call and contact), readable through
`chatRoomDeclarations(roomId)` as on the Rust `MockPlatform`, and a deleted room leaves the room list.
