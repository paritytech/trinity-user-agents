---
"@parity/truapi-host": major
"@parity/truapi": patch
---

After a chain is wiped, the same endpoint serves a new chain. The core now checks for that: it asks each chain
connection for its genesis hash before using it. If the hash isn't the one in the stack's descriptor, the core drops the
connection, calls to that chain fail, and the chain shows up in the new `StackStatus.mismatches`.

Breaking: a build with an old descriptor can't reach a wiped chain until it has the new descriptor. Pass
`fetchStackDescriptor` so the core can get it. While a chain is listed the core asks every five minutes instead of every
hour. A host that only passes `stackChanged` still hears when `mismatches` changes.

An unreachable chain isn't listed, and a runtime built from `testStack` isn't checked.
