---
"@parity/truapi-host": patch
---

The browser core compiles the core database store and the durable transaction, NFT and NFT purse modules over it. No web host opens a database, so it behaves as before: NFT purse calls answer `Unsupported` and funding sessions are not kept. The web WASM bundle grows by about 0.35 MB compressed with brotli, mostly SQLite.
