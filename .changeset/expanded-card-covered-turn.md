---
"@parity/truapi": major
"@parity/truapi-host": major
---

The expanded card calls are named after what they do. `expandedCard.setFaceShown({ shown })` is `expandedCard.setCardCovered({ covered })`, and `covered` is the opposite of `shown`: `setCardCovered({ covered: true })` sends touches over the card to the page. `expandedCard.setBackShown({ shown })` is `expandedCard.turnCard({ side })`, with `side` set to `"Front"` or `"Back"`. Products rename the calls and invert the flag they passed to `setFaceShown`.

A Pocket card definition's `faceShown` is `startsCovered`, again inverted, and defaults to `false`. Hosts no longer read `faceShown`, so a manifest that still sets it opens the card uncovered.

Calls from products built against an earlier SDK keep working: their requests arrive as v0.1 and the core converts them. Their card definitions still need `startsCovered`.

Hosts implement `setExpandedCardCovered(covered)`, which answers an `ExpandedCardCoverOutcome`, and `turnExpandedCard(side)`, which takes a `PocketCardSide`. `PocketCardSide` is now a protocol type, so TypeScript hosts get it from `@parity/truapi`.
