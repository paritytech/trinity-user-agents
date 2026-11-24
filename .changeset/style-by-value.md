---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Adds eight modifiers that style a node by value, appended after `BlendingMode` at indices 12 to 19: `Fill`, `Shadow`,
`Translate`, `Font`, `FadeWithPan`, `Shimmer`, `Spin` and `ExitFade`. Colours are `0xRRGGBBAA` and lengths hundredths of a
logical pixel. A `Gesture` may also carry a `feel`: its children's press scale, the spring that takes them home and the
pop at the end of a completed pan. A host built before this release cannot draw a tree that uses them.
