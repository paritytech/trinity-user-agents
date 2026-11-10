---
"@parity/truapi-host": major
---

The core keeps what it stores apart per stack. The `coreStorage` callbacks take a `StackStorageKey`,
`{ stack, slot }`, where `slot` is the `CoreStorageKey` they took before and `stack` is the stack's identity: its key in
hex. A host stores each value under the whole key, so a fork and its source share nothing although their genesis hashes
are equal. `encodeCoreStorageKey` takes the same `StackStorageKey`, and `describeCoreStorageKey` decodes it and returns
`stack` beside `kind` and the owner. State stored under the previous keys is not read again: users pair once more and
permissions prompt again.
