---
"@parity/truapi-provider": patch
---

The bundled network catalog takes each chain's genesis hash from its bundled chain spec, so the catalog resolves a chain restarted from genesis by the hash of its refreshed spec.
