---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Adds a shaped tip for mask strokes. `MaskShape::Stamped { width, brush, points }` (index 3) stamps copies of a `brush`
along a line: an ellipse or a capsule as long as the width, with an aspect, an angle and the spacing between stamps.
`MaskFollow` gains an optional `brush`, so the host draws a followed pan's strokes with that tip; absent, they stay
round. `cleared_share` counts the stamps, and `MaskBrush::stamps` says where they fall. A host built before this release
cannot draw a tree that uses them.
