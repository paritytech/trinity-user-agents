---
"@parity/truapi-host": major
---

A host build can add to the core's reserved values. `ProductRuntimeConfig.stack.reservedValues` and the test host's
`testStack.reservedValues` take `allowed`, whether the stack may name a reserved value, and `extraGenesisHashes` and
`extraTlds`, which the build reserves besides Polkadot's genesis hashes and the `dot` TLD. The core refuses a stack that
names a reserved value of either list unless `allowed` is set, and a build cannot take a value off the core's list.
