---
"@parity/truapi-host": major
---

A host build can add to the core's reserved values. `stack.reservedValues` and the test host's
`testStack.reservedValues` take `allowed`, whether the stack may name a reserved value, and the `extraGenesisHashes` and
`extraTlds` the build reserves on top of Polkadot's genesis hashes and the `dot` TLD. A build cannot remove a value from
the core's list.
