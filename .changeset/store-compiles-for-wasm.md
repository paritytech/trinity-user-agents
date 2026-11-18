---
"@parity/truapi-host": patch
---

The browser core compiles the core database store and the durable transaction, NFT and NFT purse modules over it. It opens an in-memory core database when it is built, so public profile records and funding sessions are kept until the page reloads. NFT purse calls still answer `Unsupported`. The web WASM bundle grows by about 0.44 MB compressed with brotli (1.24 MB raw), mostly SQLite.
