---
"@parity/truapi-host": patch
---

The test host reads back the user's mute and archive state of a room with `roomUserState(roomId)`, and deleting a room
drops its state.
