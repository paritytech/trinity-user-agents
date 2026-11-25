---
"@parity/truapi": major
"@parity/truapi-host": major
---

The expanded card calls are named after what they do. `expandedCard.setFaceShown({ shown })` is `expandedCard.setCardCovered({ covered })`, and `covered` is the opposite of `shown`: `setCardCovered({ covered: true })` sends touches over the card to the page. `expandedCard.setBackShown({ shown })` is `expandedCard.turnCard({ side })`, with `side` set to `"Front"` or `"Back"`. Products rename the calls and invert the flag they passed to `setFaceShown`.

A Pocket card definition's `faceShown` is `startsCovered`, again inverted, and defaults to `false`. Hosts no longer read `faceShown`, so a manifest that still sets it opens the card uncovered.

Until a product updates, its cover calls do the reverse of what it asks: the requests keep their method ids and encoding, so the old `setFaceShown` arrives as `setCardCovered` with the opposite meaning. Its `setBackShown` still turns the card as before.

Hosts implement `setExpandedCardCovered(covered)`, which answers an `ExpandedCardCoverOutcome`, and `turnExpandedCard(side)`, which takes a `PocketCardSide`. `PocketCardSide` is now a protocol type, so TypeScript hosts get it from `@parity/truapi`.
