---
"@parity/truapi": patch
---

Document the layout rules every host draws a renderer tree by: padding goes around a fixed size, a `Box` places its children at the top start, stacks add no gap, a `Spacer` is flexible, borders sit inside the node, a shaped background clips its content, an image's natural size is one pixel to a point, and images may be PNG, JPEG, WebP or SVG.
