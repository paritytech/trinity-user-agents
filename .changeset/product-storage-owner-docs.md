---
"@parity/truapi-host": patch
---

Document that `ProductStorage.read` can be handed another product's key. On a granted cross-product read the key names the owner, not the caller, so a host that keeps a separate store per product must read from the owner the key names; one that keys a single store by the whole key needs no change.
