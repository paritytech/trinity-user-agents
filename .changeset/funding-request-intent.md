---
"@parity/truapi": major
---

`funding.request` takes `{ direction, amount? }` and answers at once with `{ intent }` while the host shows the overlay; `funding.statusSubscribe` takes `{ intent }`. A session the user dismisses ends its watchers as `Failed { Cancelled }`. `HostFundingError` is `NotConnected`, `InvalidAmount` or `Unknown`.
