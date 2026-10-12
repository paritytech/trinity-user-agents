---
"@parity/truapi-host": major
---

Native hosts keep issued allowance keys across restarts in protected platform storage, as `CoreStorageKey::NativeAllowanceKey` records listed through `coreStorageKeys`. A host that matches exhaustively over `CoreStorageKey` must handle the new key. Product reset deletes the product's records, and `clearWalletState` deletes a removed wallet's records.
