---
"@parity/truapi": patch
---

`truapi-host funding-check` follows a session to `Delivered`: a stand-in top-up checks each claim (the key controls the deposit account, the amount is within the CASH that landed) and reports it finalized without moving coins, since the CLI has no coinage engine.
