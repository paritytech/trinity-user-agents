---
"@parity/truapi": patch
---

`createTransaction` and ring VRF resolution wait out a lost chain connection on the same pauses as a halt: a node that cannot be reached, a connection that closes under the read at any step down to the metadata download, or a follow that drops. A provider that refuses the chain, such as a host that does not serve it, is still reported at once, and so is a node that answers with an error on a live connection.
