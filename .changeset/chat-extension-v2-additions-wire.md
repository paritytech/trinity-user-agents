---
"@parity/truapi": minor
---

Add the Chat Extension v2 additions to `chat`. `setRoomFeatures` declares the optional features of a room (call
buttons, HOP attachments), `setRoomCall` shows or removes the call of a room, and `roomEventsSubscribe` streams what the
user does with these features: calls. Every earlier chat request, item and event keeps its version and its
bytes, so a product built on this client keeps each earlier call working on a host that knows only the earlier surface.
