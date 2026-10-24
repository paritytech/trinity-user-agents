---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Every connection the runtime opens to a chain shares the output of the runtime calls that depend only on the runtime, such as the metadata. Products that start together and the transaction builder of the host download the 1 to 1.5 MB metadata of each runtime once between them, keyed by the hash of the runtime code at the block they ask about. A product that asks while the download is on its way waits for it, and is told the operation is inaccessible if that download fails, so it asks again.
