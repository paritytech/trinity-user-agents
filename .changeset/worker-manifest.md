---
"@parity/truapi": minor
---

The core reads a product's Worker manifest, v1 or v2, from dotNS and caches it for a day like the root manifest. `worker_manifest(product_id)` on the pairing and signing host runtimes, and natively, answers its entrypoint, its Pocket, Chat and Input flags, and its `includes.funding` configuration with unrecognised values left out.
