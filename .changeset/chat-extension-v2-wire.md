---
"@parity/truapi": major
"@parity/truapi-host": major
---

Add the Chat Extension v2 methods and types to `chat`. A product changes a room with `setRoomMetadata` (name,
description, icon and the text of an empty room), `setRoomPreview` (sort time, badge, which can be an image, pinning,
the preview line or an activity line such as "Ben is typing"), `deleteRoom` and `updateFeed`, which also deletes a
message. The preview line comes from a `ChatPreviewMessage`, a message without reactions that names the message it
replies to by id. The host asks a product questions that the product answers through `onMessagesQuery`, `onUserAction` and
`onReactionsQuery`. Each handler returns one result, `ok` with the answer or `err` with the failure. A user action can
delete a message. A message body can be `Custom`: the product draws it through `Renderer::render`, like a custom message
it posts with `postMessage`.

These methods are new in v0.2, so a v0.1 client cannot call them. Their errors name each failure and have no catch-all:
a host failure arrives as `HostFailure`, and a request or an answer that does not pass the core's checks as
`MalformedFrame`. A user action in a room that does not keep its messages in the product fails with `NotAvailable`.

The host makes room ids. `createRoom` version 2 takes `roomIdSource`, the product's own name for the room, and `config`,
the parts of the room the product serves (its messages, who reacted and the reports of what the user saw). It answers
`roomId`, the blake2b-256 hash of the product and that source. Every later call names the room with that `roomId`, so a
product that names a room with its own string must keep the id that `createRoom` returns. A host's `createChatRoom`
callback receives the derived `roomId` and answers only the registration status. Each `createRoom` replaces the
configuration of the room, also for a room that exists, so a product declares its rooms again each time it starts.
`postMessage` and `setRoomFooter` keep their shapes and versions, so a product built against the earlier client calls
them the same way through the client a host injects.