---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native hosts keep the Pocket collection in the core database. The core stores which cards are in the Pocket and the newest face of each, and serves product Pocket calls from it. `HostRuntimeConfig.pinned_pocket_cards` (`pinnedPocketCards` in Swift and Kotlin) lists the cards the host pins. The runtime gains `pocketCards()`, a stream of every card, pinned first, and `addPocketCard`, `removePocketCard` and `pocketCardFace`. The core keeps the newest face of a card from each Pocket card render, at most every two seconds and once more when the render stops. `PocketHostBridge`, the `pocket` argument of `openProductExecution` and `notifyPocketCardsChanged` are removed, `encodeRendererNode` and `decodeRendererNode` are removed, and `NativeRendererSubscription` is now `NativeSubscription`.
