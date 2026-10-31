---
"@parity/truapi": major
"@parity/truapi-host": major
---

Add the Chat Extension v2 methods and types to `chat`. A product declares which parts of a room it serves with
`setRoomConfig` (method 7), and changes a room with `setRoomMetadata` (name, description, icon and the text of an empty
room), `setRoomPreview` (sort time, badge, which can be an image, pinning, the preview line or an activity line such as
"Ben is typing"), `deleteRoom` and `updateFeed`, which also deletes a message. The host asks a product questions that
the product answers through `onMessagesQuery`, `onUserAction` and `onReactionsQuery`, with one item each. A user action
can delete a message. A message body can be `Custom`: the product draws it through `Renderer::render`, like a custom
message it posts with `postMessage`.

The host makes room ids. `createRoom` version 2 takes `roomIdSource`, the product's own name for the room, and answers
`roomId`, the blake2b-256 hash of the product and that source. Every later call names the room with that `roomId`, so a
product that names a room with its own string must keep the id that `createRoom` returns. A host's `createChatRoom`
callback receives the derived `roomId` and answers only the registration status. `postMessage` and `setRoomFooter` keep
their shapes and versions, so a product built against the earlier client calls them the same way through the client a
host injects.
