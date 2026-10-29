---
"@parity/truapi": major
---

A product can dock one of its own cards in the footer of a room it created by setting the footer to `Card` with a `ChatCustomMessage`, and replaces or clears it by setting another footer. The host draws the card through the product's renderer with a new `ChatRoomFooter` render context, and a press inside it reaches the product's `rendererActionSubscribe` with that context.

A TypeScript caller of `setRoomFooter` now passes the footer as a tagged value, for example `{ tag: "TextInput" }`.
