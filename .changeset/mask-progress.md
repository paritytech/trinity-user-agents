---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Adds progress options to masks. `ClearedShare` gains `whileDown`, so the host also checks the share while the followed
finger is down and sends the action mid-stroke, and `rescue { afterMs, fromPermille }`, which sends it when the cover has
stood still for that long above a lower share. `Mask` gains `haptics { everyPermille, step, reach }`: a `Haptic` each
time the cover passes another step, and one when the share is reached. `PanFeel` gains `pressScaleYPermille`, so a press
can scale each axis apart. A host built before this release cannot draw a tree that uses them.
