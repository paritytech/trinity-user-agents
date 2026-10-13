---
"@parity/truapi-host": minor
---

Three additions to the test host:

- A preimage lookup is live. `seedPreimage` delivers the value to lookups already open for its key, once, so a product that subscribed before the seed receives it. The Rust `MockPlatform::insert_preimage` does the same.
- Product storage can be seeded. `productStorage` on the fixture and the host page config holds entries before the product loads, for state a product reads once at startup, and a reload starts from it again. `seedProductStorage(key, value, productId?)` on the host page and the fixture writes an entry while the product runs. Both take the product's own key; the core builds the stored key through the new `productStorageKey` WASM export. `MockHost.writeProductStorage` and `MockPlatform::write_product_storage` store an entry under the core's key without counting as a host call.
- `createTestHostServer` accepts `networks` and `chain` and expands them the way the fixture does, with explicit `mock` and `runtimeConfig` values winning. They need `productUrl`, as do `loopbackStatements` and `productStorage`; without one the server refuses them. It refuses a `uri` account as the fixture does.
