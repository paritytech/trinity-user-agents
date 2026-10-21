---
"@parity/truapi": major
"@parity/truapi-host": major
---

Add the Chat Extension v2 surface to `chat`. A product declares which parts of a room it serves in the configuration of `createRoom`, and changes a room with `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed`. The host asks a product questions with five queries a product answers through `onMessagesQuery`, `onUserAction`, `onReactionsQuery`, `onParticipantsQuery` and `onMessageActionsQuery`, each with one item. `createRoom` now takes `{ roomId, metadata, config, preview }` in place of `{ roomId, name, icon }`, `setRoomFooter` takes a footer of `Input`, `Empty` or `Custom`, and `actionSubscribe` items carry an `event` in place of a `payload`, with posted message ids and the new events. Products built against the earlier client keep working, because the host answers each call in the version it was made in. A host that serves chat receives the new `createRoom` request and the `https` icon of an earlier caller as `iconUrl`.
