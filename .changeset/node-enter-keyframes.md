---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Adds motion the host plays on its own clock for any node. `Enter { from, motion, delayMs }` (modifier index 21) plays a
node in from a pose of opacity, scale, offset and rotation when it appears. `Animate(Track { property, play })` (index
22) moves the node's opacity, scale, x or y offset, or rotation, either through `Keyframes` with curves, repeats and
alternate runs, or by following a `target` that a later tree changes, on a timed curve or a spring. `Curve::ease` and
`Keyframes::value_at` are the reference both hosts follow. A host built before this release cannot draw a tree that uses
them.
