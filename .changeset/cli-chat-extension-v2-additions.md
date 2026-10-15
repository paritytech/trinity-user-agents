---
"@parity/truapi": minor
---

The `truapi-host dev` chat page serves the foundation of the Chat Extension v2 additions. The in-memory chat host lists
the features it serves in `capabilities` and shows them, with both attachment bounds, as a Capabilities line on the
page. It keeps each room's declared features whole (the room header marks a feature it does not serve), the meta of
each message, the call of each room, the personas of the product and the user's mute and archive state, which the
page's `userState` operation sets and sends to the product as `UserStateChanged`.
