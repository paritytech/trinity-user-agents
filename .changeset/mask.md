---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Adds a `Mask` modifier (index 20) that hides part of any node, or shows only part of it, with shapes: the whole node,
circles and round-capped strokes. `ExtendMask { key, shapes }` at node index 14 adds shapes to a keyed mask until the
next tree. A mask can follow a location pan, so the host draws the finger's strokes into it in the frame of the touch,
fade to a new tree's shapes, and name a `clearedShare { atPermille, action }`: the host sends the action once the shapes
cover that share of the node, checked when the followed pan lifts. A pan with `location` set reports the finger's place
inside the gesture instead of its travel, and a free one starts at the touch-down. The TS client gains `extendMask`,
`maskCircle` and `maskStroke`. A host built before this release cannot draw a tree that uses them.
