---
"@parity/truapi-host": major
---

The core keeps what it stores apart per stack. The `coreStorage` callbacks and `encodeCoreStorageKey` take a
`StackStorageKey`, and `describeCoreStorageKey` returns its `stack`. A host stores each value under the whole key.

State a host stored under the previous keys is kept. A web host serves it through `readUnscopedCoreStorage`,
`writeUnscopedCoreStorage` and `clearUnscopedCoreStorage`, keyed by `CoreStorageKey.enc(slot)` as before, and
`describeCoreStorageKey` describes those keys too, without a `stack`. The first stack to run after the update takes that
state, so a host that ran one stack keeps its session, device key, permissions and the wallet's replay ledger. Other
stacks never see it. A host that keeps a slot under a fixed key of its own, as a session shared between origins, has to
put the stack in that key too.
