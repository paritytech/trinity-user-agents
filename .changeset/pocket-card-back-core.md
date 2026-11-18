---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

The core serves the card back. `expandedCard.setBackShown` reaches the host through the new `HostBridge.setExpandedCardBackShown`, which defaults to `unsupported` and answers with the new `ExpandedCardTurnOutcome`. The core keeps a card's back beside its front, and `pocketCardFace` takes a side that the Kotlin and Swift wrappers default to the front. A product connection receives a press on a back only once it has drawn that back. The core empties the bytes of an inline image a host must not draw, in rendered trees and in face files, so it draws as empty space.
