---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the Chat Extension v2 surface to `chat`. A product declares which parts of a room it serves with `setRoomConfig` (method 31), and changes a room with `setRoomMetadata`, `setRoomPreview`, `deleteRoom` and `updateFeed`. The host asks a product questions with queries a product answers through `onMessagesQuery`, `onUserAction` and `onReactionsQuery`, each with one item. `createRoom`, `postMessage` and `setRoomFooter` keep their shapes and versions, so a product built against the earlier client calls them the same way through the client a host injects. A room the product declares no configuration for is a room the host stores. A host that serves chat implements `setChatRoomConfig`.
