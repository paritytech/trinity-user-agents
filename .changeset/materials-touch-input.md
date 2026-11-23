---
"@parity/truapi": minor
---

Renderer trees take touch. A `Touch` node takes the finger over its children and fires its triggers as actions with an empty payload: a tap in a region, a drag from one region to another, or a trail covering a share of the node. The `Materials` nodes inside it read the finger through three new arguments the host fills, `Parameter::Touch`, `Parameter::TouchState` and `Parameter::Elapsed`, and the trail through `ImageRef::Trail`, as the Materials RFC defines.
