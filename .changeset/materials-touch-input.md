---
"@parity/truapi": major
---

Renderer trees take touch. A `Touch` node takes the finger over its children and fires its triggers as actions with an empty payload: a tap in a region, a drag from one region to another, or a trail covering a share of the node. The `Materials` nodes inside it read the finger through three new arguments the host fills, `Parameter::Touch`, `Parameter::TouchState` and `Parameter::Elapsed`, and the trail through `ImageRef::Trail`, as the Materials RFC defines.

A `Materials` node draws past its bounds by `MaterialsProps::bleed` logical pixels on every side, at most 64, over the rest of the tree. A `Touch` node keeps a replay clock with `TouchProps::replay`, which a `Parameter::Replay` argument reads: it starts at the first draw and starts again when a finger goes down once the story has played out. Text takes a medium weight with `TypographyStyle::BodyMediumMedium`.
