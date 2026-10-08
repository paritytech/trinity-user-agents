---
"@parity/truapi-host": patch
---

Answer a Widget's `expandedCard.setFaceShown` with `Unsupported`, since a browser host has no expanded cards. The
generated host-callback types include `ExpandedCardHost`, the hook native hosts implement; a browser host supplies
nothing for it. `HostCallbacks` gains a required `setExpandedCardFaceShown`, so a native embedder that implements it
directly, rather than through `HostBridge`, has to add it; `HostBridge` defaults it to `Unsupported`.
