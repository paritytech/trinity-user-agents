---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

The core serves the card back. `expandedCard.setBackShown` reaches the host through the new `HostBridge.setExpandedCardBackShown`, which defaults to `unsupported`, and `ExpandedCardFaceOutcome` gains `noBack`. The core keeps a card's back beside its front, and `pocketCardFace` takes a side that the Kotlin and Swift wrappers default to the front. A press on a back reaches the product only once that product has drawn the back. `inlineImageAllowed` tells a host whether the bytes of an inline image may be drawn.
