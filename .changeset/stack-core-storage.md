---
"@parity/truapi-host": major
---

The core keeps what it stores apart per stack. The `coreStorage` callbacks take a `StackStorageKey`,
`{ stack, slot }`, where `slot` is the `CoreStorageKey` they took before and `stack` is the stack's identity: its key in
hex. A host stores each value under the whole key, so a fork and its source share nothing although their genesis hashes
are equal. `encodeCoreStorageKey` takes the same `StackStorageKey`, and `describeCoreStorageKey` decodes it and returns
`stack` beside `kind` and the owner.

State a host stored under the previous keys is kept. On a miss the core asks `readUnscopedCoreStorage(slot)` for a value
held under the slot alone, writes it under the stack's key and calls `clearUnscopedCoreStorage(slot)`. A web host
implements both: return what it stored for that `CoreStorageKey` before, and remove it. A headless host that leaves them
out is taken to hold nothing. The first stack to read a slot takes its value, so a host that ran one stack keeps its
session, device key, permissions and the wallet's replay ledger across the update. A host that keeps a slot under a
fixed key of its own, as a session shared between origins, has to put the stack in that key too.
