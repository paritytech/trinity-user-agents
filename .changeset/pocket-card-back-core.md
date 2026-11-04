---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

The core serves the card back. `expandedCard.setBackShown` reaches the host through the new `HostBridge.setExpandedCardBackShown`, which defaults to `unsupported`, and `ExpandedCardFaceOutcome` gains `noBack`. The core keeps a card's back beside its front, and `pocketCardFace` takes a side that the Kotlin and Swift wrappers default to the front. A product connection receives a press on a back only once it has drawn that back. The native bindings export `inlineImageAllowed`, which tells a host whether the bytes of an inline image may be drawn.
