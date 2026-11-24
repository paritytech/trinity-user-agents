---
"@parity/truapi-host": minor
---

A refresh can't change a stack's TLD, because accounts derive under it. A fetched copy on another TLD than the bundled
descriptor is refused: `lastRefresh` is `refused` and `restartRequired` stays unset. A stored copy on another TLD is
ignored, and the next copy the core takes replaces it.
