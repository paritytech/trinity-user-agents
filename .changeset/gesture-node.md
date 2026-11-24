---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Adds `RendererNode::Gesture` at variant index 12, a container whose children the person can tap or pan. A pan reports
start, moves and end to the worker as a `GestureEvent`, or the host drags the children itself and settles them home,
in place or at the end. A keyed gesture can be moved between trees by a `SetOffset { key, x, y }` item at index 13, which
the host keeps until the next tree. Positions and offsets are in hundredths of a logical pixel. The TS client gains
`encodeGestureEvent`, `decodeGestureEvent` and `setOffset`. A host built before this release cannot draw a tree that uses them.
