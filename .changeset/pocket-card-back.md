---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Adds the back of a Pocket card. `RenderContext` gains `PocketCardBack`, the context a product draws a card's back on and receives presses on it under. A product built before this release cannot decode it, so a product declares `back` only once it is on this release. `expandedCard.setBackShown` lets the widget under an opened card turn it to its back or its front, and a host that cannot turn cards answers `Unsupported`, as every host does in this release.
