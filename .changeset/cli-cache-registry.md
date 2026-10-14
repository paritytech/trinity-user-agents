---
"@parity/truapi": minor
---

`truapi-host` can read its cache provider set from the provider registry contract on Asset Hub. `TRUAPI_CACHE_REGISTRY`
names the contract. The host reads `list()` at the newest finalized block through `ReviveApi_call`, from the Asset Hub
of the network or from `TRUAPI_CACHE_REGISTRY_RPC`, at start and every 60 s, and keeps the last set when a read fails.
It uses the active records with protocol 1 and an `https://` API URL, as the cache nodes do. `/cache` shows the
contract, the block and the last error.
