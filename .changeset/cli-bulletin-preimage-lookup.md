---
"@parity/truapi": patch
---

The CLI host looks preimages up on the network's Bulletin node by CID (`bitswap_v1_get`) instead of an in-process map that nothing wrote to, so a product can retrieve a blob another host submitted. A miss is polled until the blob lands, a request the node can never answer ends the lookup with an error, and every value is checked against its key.
