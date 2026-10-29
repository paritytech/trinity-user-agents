---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native host runtimes list every stored permission decision across products with `permissionAuthorizations()`, read from the keys the host reports through the new `HostCallbacks::core_storage_keys`, and read or update one product's decision without opening it (`permissionAuthorizationStatus(productId, request)`, `setPermissionAuthorizationStatus(productId, request, status)`). Every write or clear of a stored decision reaches the runtime's callbacks as `permission_authorizations_changed(product_id)`. The Swift and Kotlin `HostBridge` default the notice to a no-op and `coreStorage.keys()` to a rejection.
