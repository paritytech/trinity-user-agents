---
"@parity/truapi-host": minor
---

Hosts reach the Chat Extension v2 additions through the provider: `chatSearchQuery` asks a product to search a room it
keeps (10 s by default), `publishChatRoomEvent` gives an event to the product's `room_events_subscribe` stream,
`wakeProduct` holds the product's worker for a wake and answers `Held { untilMs }` or `BudgetSpent`, and
`stageChatAttachment` is the seam for attachments stored as preimages, which the core refuses until it stages them. The
page host carries each to the Web Worker runtime.

The test host serves the additions: `createMockHost` lists every feature name (or `chatFeatures` of its config), keeps
the features, call and personas of each room, and plays mute and archive with `setRoomUserState`;
`host.chatUser` presses call buttons, answers, opens and ends calls, searches and wakes the product.
