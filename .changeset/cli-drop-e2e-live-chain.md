---
"@parity/truapi": patch
---

`truapi-host` no longer reads `E2E_LIVE_CHAIN`. It had no effect: every network routes exactly the People, Asset Hub and Bulletin chains it serves.
