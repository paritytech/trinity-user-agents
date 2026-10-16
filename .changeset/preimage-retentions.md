---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`Preimage.retentions` (wire id 7) gives the retentions of the cache payer of the product from the ledger of a cache
provider: for each content and provider the size, the window, the paid end, the charges, the price of a day and whether
it renews, and the state on the provider (kept, fetching, lost, ended, no answer). `Preimage.stopRetention` (wire id 8)
stops a retention that renews each day; the provider keeps the content until the end of the paid day. It needs the
permission `PreimageRetain`. The platform side is `retentions` and `stopRetention` of `PreimageReadHost`.
