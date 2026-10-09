---
"@parity/truapi": minor
---

`truapi-host` can read preimages through cache nodes before the Bulletin node. `TRUAPI_CACHE_PROVIDERS` names a
provider set file; for each read the host orders the providers by recent failures, measured latency and the content's
home nodes, and tries an unmeasured provider every fourth read. The payer key signs each request to a cache node for
that node, so no one else can take reads in the payer's name. The host checks every value against its CID and pays
the provider that served it with a delivery receipt signed by `//allowance//cache//{product}` of the signed-in account,
or by the key in `TRUAPI_CACHE_PAYER_SEED`. Without a payer, or when no cache node supplies the blob, the Bulletin node
answers as before. The host keeps its measurements across restarts in `cache-quality.json`, and `/cache`
shows them with the payer key.
