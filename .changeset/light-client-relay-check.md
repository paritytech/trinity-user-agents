---
"@parity/truapi": patch
---

`truapi-host --chain-backend light-client` rejects a stack whose relay restarted from genesis after the bundled chain specs, as it already did for People, Asset Hub and Bulletin.
