---
"@parity/truapi-host": minor
---

A signing host core with the `wasm-signing-host` feature exports `productResourceStatus(productId)`, and the web worker
runtime exposes it as `getProductResourceStatus(productId)`. It returns a JSON document of what the chains publicly hold
for a product under the active local session: its Statement Store allocation, its Bulletin authorization and its PGAS
balance, each read at the finalized block of its chain. The read changes nothing on any chain and returns public
accounts only. A core built without the export rejects with `PRODUCT_RESOURCE_STATUS_UNSUPPORTED`, which
`@parity/truapi-host/web` exports.
