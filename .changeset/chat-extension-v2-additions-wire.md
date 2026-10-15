---
"@parity/truapi": minor
---

Add the Chat Extension v2 additions to `chat`. `capabilities` answers the names of the Chat features a host serves and
its attachment bounds; a host that does not know it answers `unavailable`. `setRoomFeatures` declares the optional
features of a room (preimage attachments, search, call buttons, a thread parent, read aloud), `setMessageMeta` gives a
message a thread summary, a branch position or a reasoning flag, `setRoomCall` shows or removes the call of a room, and
`roomEventsSubscribe` streams what the user does with these features: mute and archive, wakes, calls and branches. A
product answers in-room searches through `onSearchQuery`. Every earlier chat request, item and event keeps its version
and its bytes, so a product built on this client keeps each earlier call working on a host that knows only the earlier
surface.
