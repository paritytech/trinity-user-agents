---
"@parity/truapi-host": major
---

The host runtime gains `clearProductState(productId)`, which forgets one product: its grants, Allow once answers and keys, and the values it wrote to product storage. Storage is cleared through a new required callback, `productStorage.clearProduct(productId)`, which removes every value the product owns and is idempotent.

Every key the core hands a host for a product's values starts with `truapi:product-storage:v1:<UTF-8 byte length of the id>:<id>:`. A host with one shared store removes the keys with that prefix. A host that keeps a store per product drops that product's store.

The Swift and Kotlin bridges gain `clearProductState(productId:)` on `TrUAPIHostRuntime` and `clearProduct(productId:)` on `HostStorageBackend` and `HostStorage`. A storage backend that does not override `clearProduct` rejects the call, so `clearProductState` fails on that host.
