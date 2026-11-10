---
"@parity/truapi-host": major
---

The core keeps what it stores apart per stack. The `coreStorage` callbacks take a `StackStorageKey`,
`{ stack, slot }`, where `slot` is the `CoreStorageKey` they took before and `stack` is the stack's identity: its key in
hex. A host stores each value under the whole key, so a fork and its source share nothing although their genesis hashes
are equal. `encodeCoreStorageKey` takes the same `StackStorageKey`, and `describeCoreStorageKey` decodes it and returns
`stack` beside `kind` and the owner. State stored under the previous keys is not read again: users pair once more and
permissions prompt again. A cached product or Worker manifest records the stack's `generation` and is not read under
another one, so a wipe drops what was read from the old chains and keeps the session and stored permissions. A
`TestStack` needs an `id` and a `generation`; `mockTestStack()` uses `mock` for both.
