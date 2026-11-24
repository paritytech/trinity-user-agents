---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Adds `Parallax { depthPermille, tilt, pan }` (modifier index 24), which moves any node by the device's tilt, by the offset
of a keyed pan's children, or both, scaled by a depth that can be negative. The tilt moves in proportion up to a clamp,
from a rest attitude that follows the device with a time constant. `Parallax::offset` is the reference both hosts
follow. A host built before this release cannot draw a tree that uses it.
