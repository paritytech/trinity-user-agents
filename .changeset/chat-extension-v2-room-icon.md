---
"@parity/truapi-host": minor
---

`screen_chat_room_icon` (`screenChatRoomIcon` in Swift and Kotlin) screens the room icon that `createRoom` or
`setRoomMetadata` carries: a resource, which is an inline image or the CID of a Bulletin preimage, or an icon that
`screen_chat_icon` accepts. A native host that keeps a typed icon builds it from a callback with this, so it accepts
each icon that the core passes on. `screen_chat_icon` keeps its rules for an icon that a host takes from a product
itself.
