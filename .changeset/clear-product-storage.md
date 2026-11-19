---
"@parity/truapi-host": major
---

The host runtime gains `clearProductState(productId)`, which forgets one product: its grants, Allow once answers and keys, and the values it wrote to product storage. Stop the product's executions first, since a value written after the clear is kept. Storage is cleared through a new required callback, `productStorage.clearProduct(productId)`, which removes every value the product owns and is idempotent. A subscription to a removed key then sees it empty.

Every key the core hands a host for a product's values starts with `truapi:product-storage:v1:<UTF-8 byte length of the id>:<id>:`. A host with one shared store removes the keys with that prefix. A host that keeps a store per product drops that product's store.

The Swift and Kotlin bridges gain `clearProductState(productId:)` on `TrUAPIHostRuntime` and `clearProductStorage(productId:)` on `HostBridge`. The core calls it on the bridge the runtime was built with, since a product that is not running has no bridge of its own. A bridge that does not override it rejects the call, so `clearProductState` fails on that host.
