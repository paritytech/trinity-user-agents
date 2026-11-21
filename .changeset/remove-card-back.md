---
"@parity/truapi": major
---

The Pocket card back is gone. `expandedCard.setBackShown` and its types are removed, and no host draws or sends `PocketCardBack`, which stays in `RenderContext` only so `ChatRoomFooter` keeps its index. A card that turns draws both its faces in its one `PocketCard` tree, under a `Pan` with `Drive` modifiers, and keeps which side is up in the product. On native hosts, `HostBridge.setExpandedCardBackShown`, `ExpandedCardTurnOutcome` and `PocketCardSide` are removed, `pocketCardFace` takes no side, and stored card backs are deleted.
