---
"@parity/truapi-host": minor
---

The core serves the Chat Extension v2 additions. A host's `ChatPlatform` callbacks gain `chatCapabilities` (the feature
names it serves; the core drops a name it does not know or cannot back), `readChatRoomFeatures` and
`setChatRoomFeatures`, `setChatRoomCall`, `readChatRoomUserStates` (the rooms the user muted or
archived) and `readChatBots` (the persona ids of the product). Each is optional: a host without them lists no feature,
reads default features, refuses the changes, and reports no muted room and no persona. The core screens every value
against the limits of the additions RFC, refuses a `bot:<id>` participant that is not a registered persona, and answers
an immediate notification into a muted room of the product with an id and shows nothing.
