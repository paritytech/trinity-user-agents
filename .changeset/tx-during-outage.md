---
"@parity/truapi": patch
---

`createTransaction` and ring VRF signing wait up to 15 seconds for a dropped chain connection or follow to come back instead of failing with `cannot select a metadata block` or `cannot load chain metadata`. A failure waiting cannot fix, such as a chain the provider does not serve, is still reported at once.
