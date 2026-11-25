---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Bounds a stamped mask line. `MaskBrush::stamps` never places stamps closer than half a logical pixel
(`MASK_STAMP_SPACING_MIN`) and never more than 4096 on one line (`MASK_STAMPS_MAX`): a longer line has its spacing
stretched to fit. A zero-width line across the whole coordinate range no longer asks a host for billions of stamps.
