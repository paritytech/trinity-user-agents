---
"@parity/truapi": patch
---

`expandedCard.setFaceShown` is documented as covering the face of the card under the calling Widget rather than moving
it away: with `shown: false` the face stays drawn and touches over it go to the Widget, and neither state moves the face
or resizes the Widget. `UserMoving` on `setFaceShown` and `setBackShown` means the user is moving or turning the card.
The wire format is unchanged.
