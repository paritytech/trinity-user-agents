---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A `CoverGrid` may name a `reveal: { pan, atPermille, action }`: the host sends `action` once the cover's cleared cells
reach `atPermille` thousandths of the grid and the finger is up on the pan keyed `pan`, so the share that reveals a
scratch card is decided by the host, not by each worker. The field changes a cover grid's bytes, so a host built before
this release cannot draw a cover grid sent by a client of this one.
