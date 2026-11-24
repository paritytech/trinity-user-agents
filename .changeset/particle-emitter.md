---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Adds `Emit(Emitter)` (modifier index 23), which has the host send and move particles from any node on its own clock.
Particles are circles, rectangles or images, start under a keyed location pan's finger, at a point or anywhere in the
node, and are sent at a rate in time or in distance the finger moves. Each has a life, a speed and direction spread,
gravity, spin, a fade and an end scale. Bursts go off when the node appears, when the host sends an action, or when a
tree changes a value. `Emitter::place_at` is the reference both hosts follow. A host built before this release cannot
draw a tree that uses it.
