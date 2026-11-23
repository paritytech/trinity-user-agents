---
"@parity/truapi": major
"@parity/truapi-host": major
---

Replace `worker.beginOperation` and `worker.endOperation` with `const claim = await worker.retain()` and `claim.release()`. Upgrade the host and guest together. Rust releases each guest's outstanding references when its connection ends. Hosts can remove their product-operation callbacks. Disposing a browser or Node host ends its session immediately.

Each worker connection can now hold up to 64 guest references. This new per-connection limit replaces the host-specific limit on pending operations. A further acquisition rejects with `TooManyReferences`. Close the reference when the work finishes.

Android and iOS restore stored operations only for their legacy JavaScript runtime. Switching to the Rust TrUAPI runtime leaves those records in storage and starts without guest references. A feature that restores durable work must acquire its own worker reference.
