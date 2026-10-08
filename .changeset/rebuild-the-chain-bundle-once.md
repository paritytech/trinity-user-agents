---
"@parity/truapi": patch
---

A chain read that selects the current block rebuilds the cached Subxt bundle
once when the chainHead follow behind it has ended, so a product sees a single
failure rather than a run of identical ones while the host's socket comes back.
The follow's end is logged as a warning, because every chain read fails until
the bundle is rebuilt.
