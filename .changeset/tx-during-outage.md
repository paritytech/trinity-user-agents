---
"@parity/truapi": patch
---

`createTransaction` and ring VRF resolution wait up to 15 seconds for a lost chain connection or follow to come back instead of failing with `cannot select a metadata block` or `cannot load chain metadata`, whichever step of reading the current block and its metadata the connection drops under. A provider that refuses the chain, such as a host that does not serve it, is still reported at once, and so is a node that answers with an error on a live connection.
