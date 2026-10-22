---
"@parity/truapi-host": minor
---

The core serves the Chat Extension v2 additions. A host's `ChatPlatform` callbacks gain `chatCapabilities` (the feature
names it serves; the core drops a name it does not know), `readChatRoomFeatures` and
`setChatRoomFeatures`, `setChatRoomCall` and `setChatRoomContact`. Each is optional: a host without them lists no
feature, reads default features and refuses the changes. The core screens every value against the limits of the
additions RFC.
