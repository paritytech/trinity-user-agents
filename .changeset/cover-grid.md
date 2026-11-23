---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Adds `RendererNode::CoverGrid` at variant index 15, a cover over its children cut into cells that the worker clears, and
`ClearCells { key, cells }` at index 16, which clears more cells on a keyed cover until the next tree. A cover may name a
`brush` the host draws its newly cleared cells with. A pan with `location` set reports the finger's place inside the
gesture instead of its travel. The TS client gains `clearCells` and `coverBits`. A host built before this release cannot
draw a tree that uses them.
