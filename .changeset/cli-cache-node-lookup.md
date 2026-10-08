---
"@parity/truapi": minor
---

`truapi-host` can read preimages through cache nodes before the Bulletin node. `TRUAPI_CACHE_NODES` lists the cache
nodes to ask, in order, and `TRUAPI_CACHE_CLIENT` names the payer in their ledger. The host checks every value against
its CID, drops values that do not match, and asks the Bulletin node when no cache node supplies the blob.
