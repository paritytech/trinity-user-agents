---
"@parity/truapi-host": patch
---

Answer a Widget's `expandedCard.setFaceShown` with `Unsupported`, since a browser host has no expanded cards. The generated
host-callback types include `ExpandedCardHost`, the hook native hosts implement; a browser host supplies nothing for it.
