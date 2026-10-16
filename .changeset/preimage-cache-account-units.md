---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`Preimage.cacheAccount` v0.2 gives the units of the prices: `readPricePerMib` (a read priced by its size) and
`retentionUnitSecs` (MiB-hours or MiB-days). The method has its own versioned error, `RemotePreimageCacheAccountError`.
A v0.1 caller gets the v0.1 answer.
